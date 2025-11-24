use darling::{FromField, FromMeta};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Field, GenericArgument, Ident, ItemStruct, LitFloat, LitStr, PathArguments, Result, Type,
    TypePath,
};

use crate::utils::{snake_to_pascal, to_screaming_snake};

/// The name of the metric attribute.
const METRIC_ATTR_NAME: &str = "metric";

/// The default separator to use between the scope and the metric name.
/// NOTE: Prometheus does not support any other separators.
const DEFAULT_SEPARATOR: &str = "_";

#[derive(FromMeta, Debug)]
#[darling(derive_syn_parse)]
pub(super) struct MetricsAttr {
    /// The scope to use for the metrics. Used as a prefix for metric names.
    scope: Option<LitStr>,
    /// If true, generates a static LazyLock with SCREAMING_SNAKE_CASE name.
    #[darling(default, rename = "static")]
    _static: bool,
}

/// A wrapper over [`prometric`] metric types, containing their type path and generic
/// arguments, if any.
///
/// ```ignore
/// # use syn::parse_str;
///
/// let counter_ty =
///     MetricType::from_path(parse_str("::prometric::Counter<u64>").unwrap()).unwrap();
/// assert!(matches!(counter_ty, MetricType::Counter("::prometric::Counter", u64)));
///
/// let guauge_ty =
///     MetricType::from_path(parse_str("Gauge").unwrap()).unwrap();
/// assert!(matches!(gauge_ty, MetricType::Gauge("Gauge", ::prometric::GaugeDefault)));
/// ```
enum MetricType {
    Counter(TypePath, Type),
    Gauge(TypePath, Type),
    Histogram(TypePath),
    Summary(TypePath),
}

impl std::fmt::Display for MetricType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Counter(_, _) => write!(f, "Counter"),
            Self::Gauge(_, _) => write!(f, "Gauge"),
            Self::Histogram(_) => write!(f, "Histogram"),
            Self::Summary(_) => write!(f, "Summary"),
        }
    }
}

impl MetricType {
    /// Extract the generic argument (if specified) from the given PathArguments.
    ///
    /// Will return error if the path arguments are of the [`PathArgumnets::Parenthesized`] kind,
    /// or if there's more than 1 argument, or if the argument is not a type argument
    fn generic_argument(args: &PathArguments) -> Result<Option<Type>> {
        match &args {
            PathArguments::None => Ok(None),
            PathArguments::AngleBracketed(generic) => {
                if generic.args.len() != 1 {
                    return Err(syn::Error::new_spanned(
                        generic,
                        "Expected a single generic argument",
                    ));
                }

                let arg = &generic.args[0];
                if let GenericArgument::Type(ty) = arg {
                    Ok(Some(ty.clone()))
                } else {
                    Err(syn::Error::new_spanned(arg, "Expected a type argument"))
                }
            }
            PathArguments::Parenthesized(_) => {
                Err(syn::Error::new_spanned(args, "Expected a generic type argument"))
            }
        }
    }

    /// Parse the metric type (and generic argument) from a path segment.
    fn from_path(mut path: TypePath) -> Result<Self> {
        let last_segment = path.path.segments.last_mut().unwrap();
        let ident = last_segment.ident.clone();

        let maybe_generic = Self::generic_argument(&last_segment.arguments)?;

        // Specifically override the generic argument of `dest`
        // This effectively replaces the same type extracted in the `maybe_generic` block
        let override_generic_arg = |ty, dest: &mut PathArguments| {
            let args = syn::parse_quote! {<#ty>};
            *dest = PathArguments::AngleBracketed(args);
        };

        // Here we convert the parsed metric type (by identifier) into a variant of this enum.
        // Additionally, we encode the generic argument as part of the stored qualified type path,
        // using the prometric type alias if the default generic argument was used, for consistency.
        //
        // For example: `prometric::Counter` is parsed the same as
        // `prometric::Counter<::prometric::CounterDefault>` and will result in a
        // `MetricType::Counte` with `prometric::Counter<::prometric::CounterDefault>` for the path,
        // and `::prometric::CounterDefault` for the generic argument
        match ident.to_string().as_str() {
            "Counter" => {
                let generic =
                    maybe_generic.unwrap_or(syn::parse_str("::prometric::CounterDefault").unwrap());
                // Ensure the stored `path` has the generic argument
                override_generic_arg(generic.clone(), &mut last_segment.arguments);

                Ok(Self::Counter(path, generic))
            }
            "Gauge" => {
                let generic =
                    maybe_generic.unwrap_or(syn::parse_str("::prometric::GaugeDefault").unwrap());
                // Ensure the stored `path` has the generic argument
                override_generic_arg(generic.clone(), &mut last_segment.arguments);

                Ok(Self::Gauge(path, generic))
            }
            "Histogram" => Ok(Self::Histogram(path)),
            "Summary" => Ok(Self::Summary(path)),
            other => Err(syn::Error::new_spanned(
                ident,
                format!("Unsupported metric type '{other}'. Use Counter, Gauge, or Histogram"),
            )),
        }
    }

    fn full_type(&self) -> &TypePath {
        match self {
            Self::Counter(path, _) |
            Self::Gauge(path, _) |
            Self::Histogram(path) |
            Self::Summary(path) => path,
        }
    }

    fn partitions_for(
        &self,
        maybe_buckets: Option<syn::Expr>,
        maybe_quantiles: Option<syn::Expr>,
    ) -> Result<Partitions> {
        match self {
            MetricType::Counter(_, _) | MetricType::Gauge(_, _) => Ok(Partitions::NotApplicable),
            MetricType::Histogram(_) => {
                if maybe_quantiles.is_some() {
                    Err(syn::Error::new_spanned(
                        maybe_quantiles,
                        "Invalid configuration for Histogram: `quantiles` is not a valid option, use `buckets` or switch to Summary.",
                    ))
                } else {
                    Ok(maybe_buckets.map(Partitions::Buckets).unwrap_or(Partitions::None))
                }
            }
            MetricType::Summary(_) => {
                if maybe_buckets.is_some() {
                    Err(syn::Error::new_spanned(
                        maybe_buckets,
                        "Invalid configuration for Summary: `buckets` is not a valid option, use `quantiles` or switch to Histogram.",
                    ))
                } else {
                    Ok(maybe_quantiles.map(Partitions::Quantiles).unwrap_or(Partitions::None))
                }
            }
        }
    }
}

/// Represents which partition for a given metric type was parsed
///
/// This is realistically only useful for Histogram (`Buckets`) and Summaries (`Quantiles`).
///
/// This enum also encodes if no partitioning was specified (`None`), or if one was specified but
/// the metric type doesn't make use of it (ie: Gauge, Counter) (as `NotApplicable`).
///
/// Currently there's no difference between `None` and `NotApplicable`, but the latter might become
/// a hard error in the future, like when specifing `bucket` with a Counter metric.
enum Partitions {
    /// No partitions specified
    None,
    /// Partitions not applicable to given metric type
    ///
    /// Examples: Gauge, Counter
    NotApplicable,
    /// Buckets of a histogram
    Buckets(syn::Expr),
    /// Quantiles of a summary
    Quantiles(syn::Expr),
}

impl Partitions {
    fn buckets(&self) -> Option<&syn::Expr> {
        match self {
            Self::Buckets(buckets) => Some(buckets),
            _ => None,
        }
    }

    fn quantiles(&self) -> Option<&syn::Expr> {
        match self {
            Self::Quantiles(quantiles) => Some(quantiles),
            _ => None,
        }
    }
}

/// A builder that builds metric definitions, initializers, accessors and accessor implementations
/// from #[metric] attributes.
struct MetricBuilder {
    identifier: Ident,
    /// The type of the metric.
    ty: MetricType,
    /// The label keys to define for the metric.
    labels: Option<Vec<String>>,
    /// The full name of the metric.
    /// = scope + separator + identifier || rename.
    full_name: String,
    /// The doc string of the metric.
    help: String,
    /// The buckets of a histogram or the quantiles of a summary.
    partitions: Partitions,
}

impl MetricBuilder {
    fn try_from(field: &Field, scope: &str) -> Result<Self> {
        let metric_field = MetricField::from_field(field)?;
        if metric_field.buckets.is_some() && metric_field.quantiles.is_some() {
            return Err(syn::Error::new_spanned(
                field,
                "The attributes `buckets` and `quantiles` are mutually exclusive",
            ));
        }

        // prometheus::Opts requires a non-empty help string
        // Here we retrieve it from the `help` argument of the `metric`,
        // falling back to the documentation of the field otherwise
        let help = metric_field.help.or_else(|| {
            field
                .attrs
                .iter()
                .find(|attr| attr.path().is_ident("doc"))
                .map(|attr| {
                    let syn::Meta::NameValue(value) = &attr.meta else {
                        return Err(syn::Error::new_spanned(attr, "Expected a doc attribute"));
                    };

                    if let syn::Expr::Lit(lit) = &value.value {
                        if let syn::Lit::Str(lit) = &lit.lit {
                            Ok(lit.value().trim().to_string())
                        } else {
                            Err(syn::Error::new_spanned(attr, "Expected a string literal"))
                        }
                    } else {
                        Err(syn::Error::new_spanned(attr, "Expected a string literal"))
                    }
                })
                .transpose()
                .ok()
                .flatten()
        });

        let Some(help) = help else {
            return Err(syn::Error::new_spanned(
                field,
                "Unable to determine `help` label for metric. Provide an explicit `help` argument to `metric` or document the field",
            ));
        };

        let metric_name = metric_field
            .rename
            .as_ref()
            .unwrap_or(&field.ident.as_ref().unwrap().to_string())
            .to_owned();

        let full_name = format!("{scope}{DEFAULT_SEPARATOR}{metric_name}");

        let Type::Path(type_path) = metric_field.ty else {
            return Err(syn::Error::new_spanned(field, "Expected a path type"));
        };

        let ty = MetricType::from_path(type_path)?;

        let partitions = ty.partitions_for(metric_field.buckets, metric_field.quantiles)?;

        Ok(Self {
            identifier: metric_field
                .ident
                .ok_or(syn::Error::new_spanned(field, "Expected an identifier"))?,
            ty,
            labels: metric_field
                .labels
                .map(|labels| labels.iter().map(|label| label.value()).collect()),
            partitions,
            full_name,
            help,
        })
    }

    fn labels(&self) -> Vec<String> {
        self.labels.clone().unwrap_or_default()
    }

    /// Build the initializer for the metric field.
    fn build_initializer(&self) -> TokenStream {
        let ident = &self.identifier;
        let help = &self.help;
        let ty = self.ty.full_type();
        let name = &self.full_name;
        let labels = self.labels();
        let partitions = &self.partitions;

        match self.ty {
            MetricType::Counter(_, _) | MetricType::Gauge(_, _) => quote! {
                #ident: <#ty>::new(self.registry, #name, #help, &[#(#labels),*], self.labels.clone())
            },
            MetricType::Histogram(_) => {
                let buckets = if let Some(buckets_expr) = partitions.buckets() {
                    quote! { Some(#buckets_expr.into()) }
                } else {
                    quote! { None }
                };

                quote! {
                    #ident: <#ty>::new(self.registry, #name, #help, &[#(#labels),*], self.labels.clone(), #buckets)
                }
            }
            MetricType::Summary(_) => {
                let quantiles = if let Some(quantiles_expr) = partitions.quantiles() {
                    quote! { Some(#quantiles_expr.into()) }
                } else {
                    quote! { None }
                };

                quote! {
                    #ident: <#ty>::new(self.registry, #name, #help, &[#(#labels),*], self.labels.clone(), #quantiles)
                }
            }
        }
    }

    fn accessor_doc(&self, labels: &[String]) -> String {
        let help = &self.help;
        let mut doc_builder = format!(
            "{help}\n\
            * Metric type: [`::prometric::{}`]",
            self.ty,
        );

        if !labels.is_empty() {
            doc_builder.push_str(&format!("\n* Labels: {}\n", labels.join(", ")));
        }

        match self.ty {
            MetricType::Counter(_, _) | MetricType::Gauge(_, _) => {}
            MetricType::Histogram(_) => {
                if let Some(buckets_expr) = self.partitions.buckets() {
                    doc_builder.push_str(&format!("\n* Buckets: {}", quote! { #buckets_expr }));
                } else {
                    doc_builder.push_str("\n* Buckets: [`::prometheus::DEFAULT_BUCKETS`]");
                }
            }
            MetricType::Summary(_) => {
                if let Some(quantiles_expr) = self.partitions.quantiles() {
                    doc_builder.push_str(&format!("\n* Quantiles: {}", quote! { #quantiles_expr }));
                } else {
                    doc_builder
                        .push_str("\n* Buckets: [`::prometric::summary::DEFAULT_QUANTILES`]");
                }
            }
        }

        doc_builder
    }

    /// Build the accessor definition and implementation for the metric field.
    fn build_accessor(&self, vis: &syn::Visibility) -> (TokenStream, TokenStream) {
        let ident = &self.identifier;
        let labels = self.labels();
        let ty = self.ty.full_type();

        let accessor_name = format_ident!("{}Accessor", snake_to_pascal(&ident.to_string()));

        let label_definitions = labels.iter().map(|label| {
            let label_ident = format_ident!("{label}");
            quote! { #label_ident: String }
        });

        let label_arguments = labels.iter().map(|label| {
            let label_ident = format_ident!("{label}");
            quote! { #label_ident: impl Into<String> }
        });

        let def_doc = format!("Accessor for the `{ident}` metric.");
        let definition = quote! {
            #[doc = #def_doc]
            #vis struct #accessor_name<'a> {
                inner: &'a #ty,
                #(#label_definitions),*
            }
        };

        let accessor_doc = self.accessor_doc(&labels);

        let label_assignments = labels.iter().map(|label| {
            let label_ident = format_ident!("{label}");
            quote! { #label_ident: #label_ident.into() }
        });

        let accessor = quote! {
            #[doc = #accessor_doc]
            #[must_use = "This doesn't do anything unless the metric value is changed"]
            #vis fn #ident(&self, #(#label_arguments),*) -> #accessor_name {
                #accessor_name {
                    inner: &self.#ident,
                    #(#label_assignments),*
                }
            }
        };

        (definition, accessor)
    }

    fn build_accessor_impl(&self, vis: &syn::Visibility) -> TokenStream {
        let ident = &self.identifier;
        let labels = self.labels();
        let ty = &self.ty;

        let accessor_name = format_ident!("{}Accessor", snake_to_pascal(&ident.to_string()));
        let label_idents = labels.iter().map(|label| format_ident!("{label}"));

        let labels_array = if labels.is_empty() {
            quote! { let labels = &[]; }
        } else {
            quote! { let labels = &[#(self.#label_idents.as_str()),*]; }
        };

        let terminal_methods = match ty {
            MetricType::Counter(_, counter_ty) => quote! {
                #vis fn inc(&self) {
                    #labels_array
                    self.inner.inc(labels);
                }

                #vis fn inc_by<V>(&self, value: V)
                where
                    V: ::prometric::IntoAtomic<#counter_ty>,
                {
                    #labels_array
                    self.inner.inc_by(labels, value.into_atomic());
                }

                #vis fn reset(&self) {
                    #labels_array
                    self.inner.reset(labels);
                }
            },
            MetricType::Gauge(_, gauge_ty) => quote! {
                #vis fn inc(&self) {
                    #labels_array
                    self.inner.inc(labels);
                }

                #vis fn dec(&self) {
                    #labels_array
                    self.inner.dec(labels);
                }

                #vis fn add<V>(&self, value: V)
                where
                    V: ::prometric::IntoAtomic<#gauge_ty>,
                {
                    #labels_array
                    self.inner.add(labels, value.into_atomic());
                }

                #vis fn sub<V>(&self, value: V)
                where
                    V: ::prometric::IntoAtomic<#gauge_ty>,
                {
                    #labels_array
                    self.inner.sub(labels, value.into_atomic());
                }

                #vis fn set<V>(&self, value: V)
                where
                    V: ::prometric::IntoAtomic<#gauge_ty>,
                {
                    #labels_array
                    self.inner.set(labels, value.into_atomic());
                }
            },
            MetricType::Histogram(_) => quote! {
                #vis fn observe<V>(&self, value: V)
                where
                    V: ::prometric::IntoAtomic<f64>,
                {
                    #labels_array
                    self.inner.observe(labels, value.into_atomic());
                }
            },
            MetricType::Summary(_) => quote! {
                #vis fn observe<V>(&self, value: V)
                where
                    V: ::prometric::IntoAtomic<f64>,
                {
                    #labels_array
                    self.inner.observe(labels, value.into_atomic());
                }
            },
        };

        quote! {
            impl<'a> #accessor_name<'a> {
                #terminal_methods
            }
        }
    }
}

#[derive(FromField)]
#[darling(attributes(metric))]
#[allow(dead_code)]
struct MetricField {
    /// The identifier of the field.
    ident: Option<Ident>,
    /// The type of the field.
    ty: Type,
    /// The name override to use for the metric.
    rename: Option<String>,
    /// The label keys to define for the metric.
    labels: Option<Vec<LitStr>>,
    /// The help string to use for the metric. Takes precedence over the doc attribute.
    help: Option<String>,
    /// The sample rate to use for the histogram.
    /// TODO: Implement this.
    sample: Option<LitFloat>,
    /// The buckets to use for the histogram.
    ///
    /// Mutually exclusive with `quantiles`
    buckets: Option<syn::Expr>,
    /// The quantiles to use for the summary.
    ///
    /// Mutually exclusive with `buckets`
    quantiles: Option<syn::Expr>,
}

pub fn expand(metrics_attr: MetricsAttr, input: &mut ItemStruct) -> Result<TokenStream> {
    let mut initializers = Vec::with_capacity(input.fields.len());
    let mut definitions = Vec::with_capacity(input.fields.len());
    let mut accessors = Vec::with_capacity(input.fields.len());
    let mut accessor_impls = Vec::with_capacity(input.fields.len());

    // The visibility of the metrics struct
    let vis = &input.vis;
    // The identifier of the metrics struct
    let ident = &input.ident;

    for field in input.fields.iter_mut() {
        let builder =
            MetricBuilder::try_from(field, &metrics_attr.scope.as_ref().unwrap().value())?;

        initializers.push(builder.build_initializer());
        let (definition, accessor) = builder.build_accessor(vis);
        definitions.push(definition);
        accessors.push(accessor);
        accessor_impls.push(builder.build_accessor_impl(vis));

        // Remove the metric attribute from the field.
        field.attrs.retain(|attr| !attr.path().is_ident(METRIC_ATTR_NAME));
    }

    let builder_name = format_ident!("{ident}Builder");

    let mut output = quote! {
        #vis struct #builder_name<'a> {
            registry: &'a ::prometheus::Registry,
            labels: ::std::collections::HashMap<String, String>,
        }

        impl<'a> #builder_name<'a> {
            /// Set the registry to use for the metrics.
            #vis fn with_registry(mut self, registry: &'a prometheus::Registry) -> Self {
                self.registry = registry;
                self
            }

            /// Add a static label to the metrics struct.
            #vis fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
                self.labels.insert(key.into(), value.into());
                self
            }

            /// Build and register the metrics with the registry.
            #vis fn build(self) -> #ident {
                #ident {
                    #(#initializers),*
                }
            }
        }

        #input
    };

    let static_decl = if metrics_attr._static {
        let static_name = format_ident!("{}", to_screaming_snake(&ident.to_string()));
        Some(quote! {
            /// A static instance of the metrics, initialized with default values.
            /// This static is generated when `static` is enabled on the `#[metrics]` attribute.
            #vis static #static_name: ::std::sync::LazyLock<#ident> = ::std::sync::LazyLock::new(|| #ident::builder().build());
        })
    } else {
        None
    };

    // When static is true, make builder() private so users must use the static LazyLock
    let builder_vis = if metrics_attr._static {
        quote! {}
    } else {
        quote! { #vis }
    };

    // When static is true, don't implement Default
    let default_impl = if metrics_attr._static {
        quote! {}
    } else {
        quote! {
            impl Default for #ident {
                fn default() -> Self {
                    Self::builder().build()
                }
            }
        }
    };

    output = quote! {
        #output

        #default_impl

        #(#definitions)*

        #(#accessor_impls)*

        impl #ident {
            /// Create a new builder for the metrics struct.
            /// It will be initialized with the default registry and no labels.
            #builder_vis fn builder<'a>() -> #builder_name<'a> {
                #builder_name {
                    registry: ::prometheus::default_registry(),
                    labels: ::std::collections::HashMap::new(),
                }
            }

            #(#accessors)*
        }
    };

    if let Some(static_decl) = static_decl {
        output = quote! {
            #output

            #static_decl
        };
    }

    Ok(output)
}
