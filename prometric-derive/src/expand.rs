use darling::{FromField, FromMeta};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Field, GenericArgument, Ident, ItemStruct, LitFloat, LitStr, PathArguments, PathSegment,
    Result, Type,
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

enum MetricType {
    Counter(Ident, Type),
    Gauge(Ident, Type),
    Histogram(Ident),
}

impl std::fmt::Display for MetricType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Counter(_, _) => write!(f, "Counter"),
            Self::Gauge(_, _) => write!(f, "Gauge"),
            Self::Histogram(_) => write!(f, "Histogram"),
        }
    }
}

impl MetricType {
    /// Parse the metric type (and generic argument) from a path segment.
    fn from_segment(segment: &PathSegment) -> Result<Self> {
        let ident = segment.ident.clone();

        // Parse the potential generic argument.
        let maybe_generic = match &segment.arguments {
            PathArguments::None => None,
            PathArguments::AngleBracketed(generic) => {
                if generic.args.len() != 1 {
                    return Err(syn::Error::new_spanned(
                        segment,
                        "Expected a single generic argument",
                    ));
                }

                let arg = &generic.args[0];
                if let GenericArgument::Type(ty) = arg {
                    Some(ty.clone())
                } else {
                    return Err(syn::Error::new_spanned(arg, "Expected a type argument"));
                }
            }
            PathArguments::Parenthesized(_) => {
                return Err(syn::Error::new_spanned(segment, "Expected a generic type argument"));
            }
        };

        match ident.to_string().as_str() {
            "Counter" => {
                // NOTE: Use the prometric type alias here so it remains consistent.
                let generic =
                    maybe_generic.unwrap_or(syn::parse_str("prometric::CounterDefault").unwrap());
                Ok(Self::Counter(ident.clone(), generic))
            }
            "Gauge" => {
                // NOTE: Use the prometric type alias here so it remains consistent.
                let generic =
                    maybe_generic.unwrap_or(syn::parse_str("prometric::GaugeDefault").unwrap());
                Ok(Self::Gauge(ident.clone(), generic))
            }
            "Histogram" => Ok(Self::Histogram(ident.clone())),
            other => Err(syn::Error::new_spanned(
                ident,
                format!("Unsupported metric type '{other}'. Use Counter, Gauge, or Histogram"),
            )),
        }
    }

    fn full_type(&self) -> TokenStream {
        match self {
            Self::Counter(ident, ty) => quote! { #ident<#ty> },
            Self::Gauge(ident, ty) => quote! { #ident<#ty> },
            Self::Histogram(ident) => quote! { #ident },
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
    /// The buckets to use for the histogram.
    buckets: Option<syn::Expr>,
    /// The full name of the metric.
    /// = scope + separator + identifier || rename.
    full_name: String,
    /// The doc string of the metric.
    help: String,
}

impl MetricBuilder {
    fn try_from(field: &Field, scope: &str) -> Result<Self> {
        let metric_field = MetricField::from_field(field)?;

        let help = metric_field
            .help
            .or_else(|| {
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
            })
            .unwrap_or_default();

        let metric_name = metric_field
            .rename
            .as_ref()
            .unwrap_or(&field.ident.as_ref().unwrap().to_string())
            .to_owned();

        let full_name = format!("{scope}{DEFAULT_SEPARATOR}{metric_name}");

        let Type::Path(type_path) = &metric_field.ty else {
            return Err(syn::Error::new_spanned(field, "Expected a path type"));
        };

        let last_segment = type_path.path.segments.last().unwrap();

        let ty = MetricType::from_segment(last_segment)?;

        Ok(Self {
            identifier: metric_field
                .ident
                .ok_or(syn::Error::new_spanned(field, "Expected an identifier"))?,
            ty,
            labels: metric_field
                .labels
                .map(|labels| labels.iter().map(|label| label.value()).collect()),
            buckets: metric_field.buckets,
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
        let buckets = &self.buckets;

        if let MetricType::Histogram(_) = &self.ty {
            let buckets = if let Some(buckets_expr) = buckets {
                quote! { Some(#buckets_expr.into()) }
            } else {
                quote! { None }
            };

            quote! {
                #ident: <#ty>::new(self.registry, #name, #help, &[#(#labels),*], self.labels.clone(), #buckets)
            }
        } else {
            quote! {
                #ident: <#ty>::new(self.registry, #name, #help, &[#(#labels),*], self.labels.clone())
            }
        }
    }

    fn accessor_doc(&self, labels: &[String]) -> String {
        let help = &self.help;
        let mut doc_builder = format!(
            "{help}\n\
            * Metric type: [prometric::{}]",
            self.ty,
        );

        if !labels.is_empty() {
            doc_builder.push_str(&format!("\n* Labels: {}\n", labels.join(", ")));
        }

        if let MetricType::Histogram(_) = &self.ty {
            if let Some(buckets_expr) = &self.buckets {
                doc_builder.push_str(&format!("\n* Buckets: {}", quote! { #buckets_expr }));
            } else {
                doc_builder.push_str("\n* Buckets: [prometheus::DEFAULT_BUCKETS]");
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
                    V: prometric::IntoAtomic<#counter_ty>,
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
                    V: prometric::IntoAtomic<#gauge_ty>,
                {
                    #labels_array
                    self.inner.add(labels, value.into_atomic());
                }

                #vis fn sub<V>(&self, value: V)
                where
                    V: prometric::IntoAtomic<#gauge_ty>,
                {
                    #labels_array
                    self.inner.sub(labels, value.into_atomic());
                }

                #vis fn set<V>(&self, value: V)
                where
                    V: prometric::IntoAtomic<#gauge_ty>,
                {
                    #labels_array
                    self.inner.set(labels, value.into_atomic());
                }
            },
            MetricType::Histogram(_) => quote! {
                #vis fn observe<V>(&self, value: V)
                where
                    V: prometric::IntoAtomic<f64>,
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
    /// The buckets to use for the histogram.
    buckets: Option<syn::Expr>,
    /// The sample rate to use for the histogram.
    /// TODO: Implement this.
    sample: Option<LitFloat>,
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
            registry: &'a prometheus::Registry,
            labels: std::collections::HashMap<String, String>,
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
            #vis static #static_name: std::sync::LazyLock<#ident> = std::sync::LazyLock::new(|| #ident::builder().build());
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
                    registry: prometheus::default_registry(),
                    labels: std::collections::HashMap::new(),
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
