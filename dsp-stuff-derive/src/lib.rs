use darling::util::{Flag, Override, SpannedValue};
use darling::{ast, FromDeriveInput, FromField, FromMeta, ToTokens};
use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, DeriveInput};

#[derive(FromMeta)]
struct SliderOptions {
    range: syn::Expr,
    logarithmic: Flag,
    #[darling(default)]
    suffix: Option<String>,
}

#[derive(FromField)]
#[darling(attributes(dsp))]
struct FieldOpts {
    ident: Option<syn::Ident>,
    ty: syn::Type,

    id: Flag,
    inputs: Flag,
    outputs: Flag,

    /// Include the field in the serialized config
    #[darling(default)]
    save: Option<Override<syn::TypePath>>,

    #[darling(default)]
    label: SpannedValue<Option<String>>,

    /// Display this field as a slider with a given range
    ///
    /// Also sets up the field as an input that maps -1.0..=1.0 to start..=end
    #[darling(default)]
    slider: Option<SliderOptions>,

    /// Display this field as a select menu
    select: Flag,

    #[darling(default)]
    default: Option<syn::Expr>,
}

#[derive(FromDeriveInput)]
#[darling(attributes(dsp), supports(struct_named))]
struct Dsp {
    ident: syn::Ident,
    data: ast::Data<darling::util::Ignored, FieldOpts>,

    title: String,
    cfg_name: String,
    description: String,

    #[darling(default)]
    custom_render: SpannedValue<Option<syn::Expr>>,

    #[darling(default)]
    after_settings_change: Option<syn::Expr>,

    #[darling(multiple, rename = "input")]
    inputs: Vec<String>,

    #[darling(multiple, rename = "output")]
    outputs: Vec<String>,
}

#[proc_macro_derive(DspNode, attributes(dsp))]
pub fn derive_dsp(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let a = match Dsp::from_derive_input(&parse_macro_input!(input as DeriveInput)) {
        Ok(a) => a,
        Err(e) => return proc_macro::TokenStream::from(e.write_errors()),
    };

    let r = match do_node(&a) {
        Ok(r) => r,
        Err(e) => return proc_macro::TokenStream::from(e.write_errors()),
    };

    r.into()
}

fn do_node(dsp: &Dsp) -> darling::Result<TokenStream> {
    let meta = do_meta(dsp);
    let getters = do_getters(&dsp.data)?;
    let render = do_render(&dsp.data, &dsp.custom_render, &dsp.after_settings_change)?;
    let (cfg_struct, save_restore) = do_save_restore(&dsp.ident, &dsp.data);
    let new = do_new(&dsp.inputs, &dsp.outputs, &dsp.data);

    let ident = &dsp.ident;
    let tokens = quote! {
        #cfg_struct

        impl crate::node::Node for #ident {
            #meta

            #getters

            #save_restore

            #render

            #new
        }
    };

    Ok(tokens)
}

fn do_new(
    inputs: &[String],
    outputs: &[String],
    data: &ast::Data<darling::util::Ignored, FieldOpts>,
) -> TokenStream {
    let fields = data.as_ref().take_struct().unwrap();
    let id_field = fields
        .iter()
        .find(|f| f.id.is_present())
        .unwrap()
        .ident
        .as_ref()
        .unwrap();
    let inputs_field = fields
        .iter()
        .find(|f| f.inputs.is_present())
        .unwrap()
        .ident
        .as_ref()
        .unwrap();
    let outputs_field = fields
        .iter()
        .find(|f| f.outputs.is_present())
        .unwrap()
        .ident
        .as_ref()
        .unwrap();

    let field_defaulters = fields.iter().filter_map(|f| {
        if f.id.is_present() || f.inputs.is_present() || f.outputs.is_present() {
            return None;
        }

        let val = if let Some(v) = &f.default {
            quote! { #v.into() }
        } else {
            quote! { ::std::default::Default::default() }
        };

        let ident = f.ident.as_ref().unwrap();

        Some(quote! { #ident: #val })
    });

    let new_defn = quote! {
        fn new(id: crate::ids::NodeId) -> Self {
            let inputs = crate::node::PortStorage::default();
            #(inputs.add(#inputs.to_owned());)*

            let outputs = crate::node::PortStorage::default();
            #(outputs.add(#outputs.to_owned());)*

            Self {
                #id_field: id,
                #inputs_field: inputs,
                #outputs_field: outputs,
                #(#field_defaulters),*
            }
        }
    };

    new_defn
}

fn do_save_restore(
    name: &syn::Ident,
    data: &ast::Data<darling::util::Ignored, FieldOpts>,
) -> (TokenStream, TokenStream) {
    let fields = data.as_ref().take_struct().unwrap();

    let struct_fields = fields
        .iter()
        .filter_map(|&f| {
            if let Some(ty) = &f.save {
                let (should_wrap, ty) = ty.as_ref().explicit().map_or_else(
                    || (false, f.ty.to_token_stream()),
                    |ty| (true, ty.to_token_stream()),
                );
                let ident = f.ident.as_ref().unwrap();

                Some((ident, ty, should_wrap, false))
            } else if f.id.is_present() || f.inputs.is_present() || f.outputs.is_present() {
                let ty = f.ty.to_token_stream();
                let ident = f.ident.as_ref().unwrap();

                Some((ident, ty, false, f.id.is_present()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let idents = struct_fields.iter().map(|(i, _, _, _)| i);

    let cfg_struct_name = quote::format_ident!("{}Config", name);

    let struct_defn = quote! {
        #[derive(::serde::Deserialize, ::serde::Serialize)]
        struct #cfg_struct_name {
            #(#idents: ::serde_json::Value),*
        }
    };

    let save_getters = struct_fields.iter().map(|(i, ty, wrap, _)| {
        if *wrap {
            quote! {
                #i: ::serde_json::to_value(<#ty>::from(&self.#i)).unwrap()
            }
        } else {
            quote! {
                #i: ::serde_json::to_value(&self.#i).unwrap()
            }
        }
    });

    let save_defn = quote! {
        fn save(&self) -> ::serde_json::Value {
            let cfg = #cfg_struct_name {
                #(#save_getters),*
            };

            ::serde_json::to_value(cfg).unwrap()
        }
    };

    let restore_setters =
        struct_fields
            .iter()
            .filter(|(_, _, _, is_id)| !*is_id)
            .map(|(i, ty, wrap, _)| {
                if *wrap {
                    quote! {
                        this.#i = ::serde_json::from_value::<#ty>(cfg.#i).unwrap().into();
                    }
                } else {
                    quote! {
                        this.#i = ::serde_json::from_value::<#ty>(cfg.#i).unwrap();
                    }
                }
            });

    let id_field = fields
        .iter()
        .find(|f| f.id.is_present())
        .unwrap()
        .ident
        .as_ref()
        .unwrap();

    let restore_defn = quote! {
        fn restore(value: ::serde_json::Value) -> Self {
            let cfg: #cfg_struct_name = serde_json::from_value(value).unwrap();

            let id = serde_json::from_value(cfg.#id_field).unwrap();
            let mut this = Self::new(id);

            #(#restore_setters)*

            this
        }
    };

    let tokens = quote! {
        #save_defn

        #restore_defn
    };

    (struct_defn, tokens)
}

fn do_meta(dsp: &Dsp) -> TokenStream {
    let title = &dsp.title;
    let cfg_name = &dsp.cfg_name;
    let description = &dsp.description;

    quote! {
        fn title(&self) -> &'static ::std::primitive::str {
            #title
        }

        fn cfg_name(&self) -> &'static ::std::primitive::str {
            #cfg_name
        }

        fn description(&self) -> &'static ::std::primitive::str {
            #description
        }
    }
}

fn generate_getter(
    name: &str,
    add_ref: bool,
    field_fn: for<'a> fn(&'a FieldOpts) -> &'a Flag,
    data: &ast::Data<darling::util::Ignored, FieldOpts>,
) -> darling::Result<TokenStream> {
    let fields = data.as_ref().take_struct().unwrap();

    let field = fields
        .iter()
        .filter(|f| field_fn(f).is_present())
        .collect::<Vec<_>>();

    match &field[..] {
        [field] => {
            let ty = &field.ty;
            let ident = field.ident.as_ref().unwrap();
            let name_ident = syn::Ident::new(name, ident.span());
            let r = if add_ref { quote!(&) } else { quote!() };
            let tokens = quote! {
                fn #name_ident(&self) -> #r #ty {
                    #r self.#ident
                }
            };
            Ok(tokens)
        }
        [] => Err(darling::Error::custom(format!(
            "Expected one field to be tagged with #[dsp({name})]"
        ))),
        _ => Err(darling::Error::multiple(
            fields
                .into_iter()
                .map(|f| {
                    darling::Error::custom(format!(
                        "Expected only one field to be tagged with #[dsp({name})]"
                    ))
                    .with_span(&field_fn(f).span())
                })
                .collect(),
        )),
    }
}

fn do_getters(data: &ast::Data<darling::util::Ignored, FieldOpts>) -> darling::Result<TokenStream> {
    let mut errors = darling::Error::accumulator();
    let mut token_pieces = Vec::new();

    let fields: &[(_, fn(&FieldOpts) -> &Flag, _)] = &[
        ("id", |f| &f.id, false),
        ("inputs", |f| &f.inputs, true),
        ("outputs", |f| &f.outputs, true),
    ];

    for (name, field_fn, add_ref) in fields {
        if let Some(tokens) = errors.handle(generate_getter(name, *add_ref, *field_fn, data)) {
            token_pieces.push(tokens);
        }
    }

    errors.finish()?;

    let tokens = quote! {
        #(#token_pieces)*
    };

    Ok(tokens)
}

fn do_render(
    data: &ast::Data<darling::util::Ignored, FieldOpts>,
    custom_renderer: &SpannedValue<Option<syn::Expr>>,
    after_settings_change: &Option<syn::Expr>,
) -> darling::Result<TokenStream> {
    let fields = data.as_ref().take_struct().unwrap();

    let mut errors = darling::Error::accumulator();

    let mut rendered_fields = fields
        .iter()
        .filter_map(|&f| {
            if !(f.slider.is_some() || f.select.is_present()) {
                return None;
            }

            if f.slider.is_some() && f.select.is_present() {
                errors.push(
                    darling::Error::custom("A field cannot be both a slider and a select")
                        .with_span(&f.select.span()),
                );
            }

            let ident = errors.handle(
                f.ident
                    .as_ref()
                    .ok_or_else(|| darling::Error::custom("I need a named attribute")),
            )?;

            let label = f
                .label
                .as_ref()
                .to_owned()
                .unwrap_or_else(|| capitalize(f.ident.as_ref().unwrap().to_string()));

            let tokens = if let Some(r) = &f.slider {
                let range = &r.range;
                let suffix_expr = if let Some(suffix) = &r.suffix {
                    quote! {
                        .suffix(#suffix)
                    }
                } else {
                    quote! {}
                };

                let logarithmic_expr = if r.logarithmic.is_present() {
                    quote! {
                        .logarithmic(true)
                    }
                } else {
                    quote! {}
                };

                quote! {
                    let r = ui.add(::egui::Slider::from_get_set(#range, |v| {
                        if let ::std::option::Option::Some(v) = v {
                            self.#ident.store(v as _, ::std::sync::atomic::Ordering::Relaxed);
                        }
                        self.#ident.load(::std::sync::atomic::Ordering::Relaxed) as ::std::primitive::f64
                    }).text(#label) #suffix_expr #logarithmic_expr);

                    if r.changed() {
                        changed |= true;
                    }
                }
            } else if f.select.is_present() {
                let ty = &f.ty;

                quote! {
                    {
                        let current_selected = self.#ident.load(::std::sync::atomic::Ordering::Relaxed);
                        let mut selected = current_selected;

                        fn enum_as_iter<A: ::std::convert::From<T>, T: ::strum::IntoEnumIterator>() -> impl ::std::iter::Iterator<Item = T> {
                            T::iter()
                        }

                        ::egui::ComboBox::new((::std::stringify!(#ident), self.id()), #label)
                            .selected_text(<&'static ::std::primitive::str>::from(selected))
                            .show_ui(ui, |ui| {
                               for possible_selection in enum_as_iter::<#ty, _>() {
                                   ui.selectable_value(
                                       &mut selected,
                                       possible_selection,
                                       <&'static ::std::primitive::str>::from(possible_selection)
                                   );
                               }
                            });

                        if selected != current_selected {
                            self.#ident.store(selected, ::std::sync::atomic::Ordering::Relaxed);
                            changed |= true;
                        }
                    }
                }
            } else {
                unreachable!()
            };

            Some(tokens)
        })
        .collect::<Vec<_>>();

    if custom_renderer.is_some() && !rendered_fields.is_empty() {
        errors.push(
            darling::Error::custom(
                "Don't use rendering related fields if you're passing a custom renderer",
            )
            .with_span(&custom_renderer.span()),
        );
    }

    if custom_renderer.is_some() && after_settings_change.is_some() {
        errors.push(
            darling::Error::custom("Don't mix after_settings_change and custom_renderer")
                .with_span(&custom_renderer.span()),
        );
    }

    if let Some(f) = custom_renderer.as_ref() {
        rendered_fields.push(quote! {
            (#f)(self, ui);
        });
    }

    errors.finish()?;

    let changed_expr = if let Some(e) = after_settings_change {
        quote! {
            if changed {
                (#e)(self);
            }
        }
    } else {
        quote! {}
    };

    let tokens = quote! {
        fn render(&self, ui: &mut ::egui::Ui) {
            let mut changed = false;

            #(#rendered_fields)*

            #changed_expr
        }
    };

    Ok(tokens)
}

fn capitalize(s: String) -> String {
    let mut it = s.chars();
    if let Some(c) = it.next() {
        c.to_uppercase().collect::<String>() + it.as_str()
    } else {
        String::new()
    }
}
