#![feature(proc_macro_diagnostic)]
#![feature(allow_internal_unstable)]

extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::{Ident, Literal, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
    spanned::Spanned,
    token::Paren,
    Attribute, Error, Expr, Field, Fields, FieldsNamed, FieldsUnnamed, FnArg, ImplItem,
    ImplItemMethod, Item, ItemEnum, ItemFn, ItemImpl, ItemStruct, ItemTrait, Pat, PatIdent,
    PatType, Path, PathArguments, PathSegment, Receiver, Result, ReturnType, Signature, Token,
    TraitItem, TraitItemMethod, Type, TypePath, TypeTuple,
};

fn get_ref_ident(ident: &Ident) -> Ident {
    Ident::new(&(ident.to_string() + "Ref"), ident.span())
}

fn get_internal_function_ident(ident: &Ident) -> Ident {
    Ident::new(&(ident.to_string() + "_inline"), ident.span())
}

fn get_trait_mod_ident(ident: &Ident) -> Ident {
    Ident::new(&(ident.to_string() + "TurboTasksMethods"), ident.span())
}

fn get_slot_value_type_ident(ident: &Ident) -> Ident {
    Ident::new(
        &(ident.to_string().to_uppercase() + "_NODE_TYPE"),
        ident.span(),
    )
}

fn get_trait_type_ident(ident: &Ident) -> Ident {
    Ident::new(
        &(ident.to_string().to_uppercase() + "_TRAIT_TYPE"),
        ident.span(),
    )
}

fn get_register_trait_methods_ident(trait_ident: &Ident, struct_ident: &Ident) -> Ident {
    Ident::new(
        &("__register_".to_string()
            + &struct_ident.to_string()
            + "_"
            + &trait_ident.to_string()
            + "_trait_methods"),
        trait_ident.span(),
    )
}

fn get_function_ident(ident: &Ident) -> Ident {
    Ident::new(
        &(ident.to_string().to_uppercase() + "_FUNCTION"),
        ident.span(),
    )
}

fn get_trait_impl_function_ident(struct_ident: &Ident, ident: &Ident) -> Ident {
    Ident::new(
        &(struct_ident.to_string().to_uppercase()
            + "_IMPL_"
            + &ident.to_string().to_uppercase()
            + "_FUNCTION"),
        ident.span(),
    )
}

#[allow_internal_unstable(into_future, trivial_bounds)]
#[proc_macro_attribute]
pub fn value(args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as Item);
    let traits = if args.is_empty() {
        Vec::new()
    } else {
        parse_macro_input!(args with Punctuated<Ident, Token![+]>::parse_terminated)
            .into_iter()
            .collect()
    };

    let (vis, ident) = match &item {
        Item::Enum(ItemEnum { vis, ident, .. }) => (vis, ident),
        Item::Struct(ItemStruct { vis, ident, .. }) => (vis, ident),
        _ => {
            item.span().unwrap().error("unsupported syntax").emit();

            return quote! {
                #item
            }
            .into();
        }
    };

    let ref_ident = get_ref_ident(&ident);
    let slot_value_type_ident = get_slot_value_type_ident(&ident);
    let trait_registrations: Vec<_> = traits
        .iter()
        .map(|trait_ident| {
            let register = get_register_trait_methods_ident(trait_ident, &ident);
            quote! {
                #register(&mut slot_value_type);
            }
        })
        .collect();
    let expanded = quote! {
        #[derive(turbo_tasks::trace::TraceSlotRefs)]
        #item

        lazy_static::lazy_static! {
            static ref #slot_value_type_ident: turbo_tasks::SlotValueType = {
                let mut slot_value_type = turbo_tasks::SlotValueType::new(std::any::type_name::<#ident>().to_string());
                #(#trait_registrations)*
                slot_value_type
            };
        }

        #[derive(Clone, Debug, std::hash::Hash, std::cmp::Eq, std::cmp::PartialEq)]
        #vis struct #ref_ident {
            node: turbo_tasks::SlotRef,
        }

        impl #ref_ident {
            #[inline]
            pub fn get_default_task_argument_options() -> turbo_tasks::TaskArgumentOptions { turbo_tasks::TaskArgumentOptions::Resolved(&#slot_value_type_ident) }

            pub fn from_slot_ref(node: turbo_tasks::SlotRef) -> Self {
                // if node.is_slot_value_type(&#slot_value_type_ident) {
                //     Some(Self { node })
                // } else {
                //     None
                // }
                Self { node }
            }

            // pub fn verify(node: &turbo_tasks::SlotRef) -> anyhow::Result<()> {
                // if node.is_slot_value_type(&#slot_value_type_ident) {
                //     Ok(())
                // } else {
                //     Err(anyhow::anyhow!(
                //         "expected {:?} but got {:?}",
                //         *#slot_value_type_ident,
                //         node.get_slot_value_type()
                //     ))
                // }
            // }

            pub async fn get(&self) -> impl std::ops::Deref<Target = #ident> {
                self.node.read::<#ident>()
            }
        }

        // #[cfg(feature = "into_future")]
        impl std::future::IntoFuture for #ref_ident {
            type Output = turbo_tasks::macro_helpers::SlotReadResult<#ident>;
            type Future = std::future::Ready<turbo_tasks::macro_helpers::SlotReadResult<#ident>>;
            fn into_future(self) -> Self::Future {
                std::future::ready(self.node.read::<#ident>())
            }
        }

        impl From<#ref_ident> for turbo_tasks::SlotRef {
            fn from(node_ref: #ref_ident) -> Self {
                node_ref.node
            }
        }

        // #[cfg(feature = "trivial_bounds")]
        impl From<#ident> for #ref_ident where #ident: std::cmp::PartialEq<#ident> {
            fn from(content: #ident) -> Self {
                Self { node: turbo_tasks::macro_helpers::match_previous_node_by_type::<#ident, _>(
                    |__slot| {
                        __slot.compare_and_update_shared(&#slot_value_type_ident, content);
                    }
                ) }
            }
        }

        // #[cfg(feature = "trivial_bounds")]
        impl #ref_ident where #ident: std::cmp::Eq + std::hash::Hash + Send + Sync + 'static {
            pub fn intern(this: #ident) -> #ref_ident {
                let arc = std::sync::Arc::new(this);
                Self { node: turbo_tasks::macro_helpers::new_node_intern::<#ident, _, _>(
                    arc.clone(),
                    || (
                        &#slot_value_type_ident,
                        arc
                    )
                ) }
            }
        }

        impl turbo_tasks::trace::TraceSlotRefs for #ref_ident {
            fn trace_node_refs(&self, context: &mut turbo_tasks::trace::TraceSlotRefsContext) {
                turbo_tasks::trace::TraceSlotRefs::trace_node_refs(&self.node, context);
            }
        }
    };

    expanded.into()
}

enum Constructor {
    Default,
    Intern,
    Compare(Option<Ident>),
    CompareEnum(Option<Ident>),
    KeyAndCompare(Option<Expr>, Option<Ident>),
    KeyAndCompareEnum(Option<Expr>, Option<Ident>),
    Key(Option<Expr>),
}

impl Parse for Constructor {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut result = Constructor::Default;
        if input.is_empty() {
            return Ok(result);
        }
        let content;
        parenthesized!(content in input);
        loop {
            let ident = content.parse::<Ident>()?;
            match ident.to_string().as_str() {
                "intern" => match result {
                    Constructor::Default => {
                        result = Constructor::Intern;
                    }
                    _ => {
                        return Err(content.error(format!("intern can't be combined")));
                    }
                },
                "compare" => {
                    let compare_name = if content.peek(Token![:]) {
                        content.parse::<Token![:]>()?;
                        Some(content.parse::<Ident>()?)
                    } else {
                        None
                    };
                    result = match result {
                        Constructor::Default => Constructor::Compare(compare_name),
                        Constructor::Key(key_expr) => {
                            Constructor::KeyAndCompare(key_expr, compare_name)
                        }
                        _ => {
                            return Err(content.error(format!(
                                "\"compare\" can't be combined with previous values"
                            )));
                        }
                    }
                }
                "compare_enum" => {
                    let compare_name = if content.peek(Token![:]) {
                        content.parse::<Token![:]>()?;
                        Some(content.parse::<Ident>()?)
                    } else {
                        None
                    };
                    result = match result {
                        Constructor::Default => Constructor::CompareEnum(compare_name),
                        Constructor::Key(key_expr) => {
                            Constructor::KeyAndCompareEnum(key_expr, compare_name)
                        }
                        _ => {
                            return Err(content.error(format!(
                                "\"compare\" can't be combined with previous values"
                            )));
                        }
                    }
                }
                "key" => {
                    let key_expr = if content.peek(Token![:]) {
                        content.parse::<Token![:]>()?;
                        Some(content.parse::<Expr>()?)
                    } else {
                        None
                    };
                    result = match result {
                        Constructor::Default => Constructor::Key(key_expr),
                        Constructor::Compare(compare_name) => {
                            Constructor::KeyAndCompare(key_expr, compare_name)
                        }
                        _ => {
                            return Err(content
                                .error(format!("\"key\" can't be combined with previous values")));
                        }
                    };
                }
                _ => {
                    return Err(Error::new_spanned(
                        &ident,
                        format!("unexpected {}, expected \"key\", \"intern\", \"compare\", \"compare_enum\" or \"update\"", ident.to_string()),
                    ))
                }
            }
            if content.is_empty() {
                return Ok(result);
            } else if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            } else {
                return Err(content.error("expected \",\" or end of attribute"));
            }
        }
    }
}

fn is_constructor(attr: &Attribute) -> bool {
    is_attribute(attr, "constructor")
}

fn is_attribute(attr: &Attribute, name: &str) -> bool {
    let path = &attr.path;
    if path.leading_colon.is_some() {
        return false;
    }
    let mut iter = path.segments.iter();
    match iter.next() {
        Some(seg) if seg.arguments.is_empty() && seg.ident.to_string() == "turbo_tasks" => {
            match iter.next() {
                Some(seg) if seg.arguments.is_empty() && seg.ident.to_string() == name => {
                    iter.next().is_none()
                }
                _ => false,
            }
        }
        _ => false,
    }
}

#[proc_macro_attribute]
pub fn value_trait(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemTrait);

    let ItemTrait {
        vis, ident, items, ..
    } = &item;

    let ref_ident = get_ref_ident(&ident);
    let mod_ident = get_trait_mod_ident(&ident);
    let trait_type_ident = get_trait_type_ident(&ident);
    let mut trait_fns = Vec::new();

    for item in items.iter() {
        if let TraitItem::Method(TraitItemMethod {
            sig:
                Signature {
                    ident: method_ident,
                    inputs,
                    output,
                    ..
                },
            ..
        }) = item
        {
            let output_type = get_return_type(&output);
            let args = inputs.iter().filter_map(|arg| match arg {
                FnArg::Receiver(_) => None,
                FnArg::Typed(PatType { pat, .. }) => Some(quote! {
                    #pat.into()
                }),
            });
            let method_args: Vec<_> = inputs.iter().collect();
            let convert_result_code = if is_empty_type(&output_type) {
                quote! {}
            } else {
                quote! { #output_type::from_slot_ref(result) }
            };
            trait_fns.push(quote! {
                pub fn #method_ident(#(#method_args),*) -> impl std::future::Future<Output = #output_type> {
                    // TODO use const string
                    let result = turbo_tasks::trait_call(&#trait_type_ident, stringify!(#method_ident).to_string(), vec![self.clone().into(), #(#args),*]).unwrap();
                    async { #convert_result_code }
                }
            })
        }
    }

    let expanded = quote! {
        #item

        lazy_static::lazy_static! {
            pub static ref #trait_type_ident: turbo_tasks::TraitType = turbo_tasks::TraitType::new(std::any::type_name::<dyn #ident>().to_string());
        }

        #vis struct #mod_ident {
            __private: ()
        }

        impl #mod_ident {
            #[inline]
            pub fn __type(&self) -> &'static turbo_tasks::TraitType {
                &*#trait_type_ident
            }
        }

        #[allow(non_upper_case_globals)]
        #vis static #ident: #mod_ident = #mod_ident { __private: () };

        #[derive(Clone, Debug, std::hash::Hash, std::cmp::Eq, std::cmp::PartialEq)]
        #vis struct #ref_ident {
            node: turbo_tasks::SlotRef,
        }

        impl #ref_ident {
            #[inline]
            pub fn get_default_task_argument_options() -> turbo_tasks::TaskArgumentOptions { turbo_tasks::TaskArgumentOptions::Trait(&#trait_type_ident) }

            pub fn from_slot_ref(node: turbo_tasks::SlotRef) -> Self {
                // if node.has_trait_type(&#trait_type_ident) {
                //     Some(Self { node })
                // } else {
                //     None
                // }
                Self { node }
            }

            // pub fn verify(node: &turbo_tasks::SlotRef) -> anyhow::Result<()> {
                // if node.has_trait_type(&#trait_type_ident) {
                //     Ok(())
                // } else {
                //     Err(anyhow::anyhow!(
                //         "expected {:?} but got {:?}",
                //         &*#trait_type_ident,
                //         node.get_slot_value_type()
                //     ))
                // }
            // }

            #(#trait_fns)*
        }

        impl From<#ref_ident> for turbo_tasks::SlotRef {
            fn from(node_ref: #ref_ident) -> Self {
                node_ref.node
            }
        }

        impl turbo_tasks::trace::TraceSlotRefs for #ref_ident {
            fn trace_node_refs(&self, context: &mut turbo_tasks::trace::TraceSlotRefsContext) {
                turbo_tasks::trace::TraceSlotRefs::trace_node_refs(&self.node, context);
            }
        }

    };
    expanded.into()
}

#[proc_macro_attribute]
pub fn value_impl(_args: TokenStream, input: TokenStream) -> TokenStream {
    fn generate_for_self_impl(ident: &Ident, items: &Vec<ImplItem>) -> TokenStream2 {
        let ref_ident = get_ref_ident(&ident);
        let slot_value_type_ident = get_slot_value_type_ident(&ident);
        let mut constructors = Vec::new();
        let mut i = 0;
        for item in items.iter() {
            match item {
                ImplItem::Method(ImplItemMethod {
                    attrs,
                    vis,
                    defaultness,
                    sig,
                    block: _,
                }) => {
                    if let Some(Attribute { tokens, .. }) =
                        attrs.iter().find(|attr| is_constructor(attr))
                    {
                        let constructor: Constructor = parse_quote! { #tokens };
                        let fn_name = &sig.ident;
                        let inputs = &sig.inputs;
                        let mut input_names = Vec::new();
                        let mut old_input_names = Vec::new();
                        let mut input_names_ref = Vec::new();
                        let index_literal = Literal::i32_unsuffixed(i);
                        let mut inputs_for_intern_key = vec![quote! { #index_literal }];
                        for arg in inputs.iter() {
                            if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
                                if let Pat::Ident(PatIdent { ident, .. }) = &**pat {
                                    input_names.push(ident.clone());
                                    old_input_names.push(Ident::new(
                                        &(ident.to_string() + "_old"),
                                        ident.span(),
                                    ));
                                    if let Type::Reference(_) = &**ty {
                                        inputs_for_intern_key
                                            .push(quote! { std::clone::Clone::clone(#ident) });
                                        input_names_ref.push(quote! { #ident });
                                    } else {
                                        inputs_for_intern_key
                                            .push(quote! { std::clone::Clone::clone(&#ident) });
                                        input_names_ref.push(quote! { &#ident });
                                    }
                                } else {
                                    item.span()
                                        .unwrap()
                                        .error(format!(
                                            "unsupported pattern syntax in {}: {}",
                                            &ident.to_string(),
                                            quote! { #pat }
                                        ))
                                        .emit();
                                }
                            }
                        }
                        let create_new_content = quote! {
                            #ident::#fn_name(#(#input_names),*)
                        };
                        let create_new_node = quote! {
                            turbo_tasks::SlotRef::SharedReference(
                                &#slot_value_type_ident,
                                std::sync::Arc::new(#create_new_content)
                            )
                        };
                        let gen_conditional_update_functor = |compare_name| {
                            let compare = match compare_name {
                                Some(name) => quote! {
                                    __self.#name(#(#input_names_ref),*)
                                },
                                None => quote! {
                                    true #(&& (#input_names == __self.#input_names))*
                                },
                            };
                            quote! {
                                |__slot| {
                                    __slot.conditional_update_shared::<#ident, _>(&#slot_value_type_ident, |__self| {
                                        if let Some(__self) = __self {
                                            if #compare {
                                                return None;
                                            }
                                        }
                                        Some(#create_new_content)
                                    })
                                }
                            }
                        };
                        let gen_compare_enum_functor = |name| {
                            let compare = if old_input_names.is_empty() {
                                quote! {
                                    if __self == Some(&#ident::#name) {
                                        return None
                                    }
                                }
                            } else {
                                quote! {
                                    if let Some(&#ident::#name(ref #(#old_input_names),*)) = __self {
                                        if true #(&& (#input_names == *#old_input_names))* {
                                            return None
                                        }
                                    }
                                }
                            };
                            quote! {
                                |__slot| {
                                    __slot.conditional_update_shared::<#ident, _>(&#slot_value_type_ident, |__self| {
                                        #compare
                                        Some(#create_new_content)
                                    })
                                }
                            }
                        };
                        let get_node = match constructor {
                            Constructor::Intern => {
                                quote! {
                                    turbo_tasks::macro_helpers::new_node_intern::<#ident, _, _>(
                                        (#(#inputs_for_intern_key),*),
                                        || (
                                            &#slot_value_type_ident,
                                            std::sync::Arc::new(#create_new_content)
                                        )
                                    )
                                }
                            }
                            Constructor::Default => {
                                quote! {
                                    #create_new_node
                                }
                            }
                            Constructor::Compare(compare_name) => {
                                let functor = gen_conditional_update_functor(compare_name);
                                quote! {
                                    turbo_tasks::macro_helpers::match_previous_node_by_type::<#ident, _>(
                                        #functor
                                    )
                                }
                            }
                            Constructor::KeyAndCompare(key_expr, compare_name) => {
                                let functor = gen_conditional_update_functor(compare_name);
                                quote! {
                                    turbo_tasks::macro_helpers::match_previous_node_by_key::<#ident, _, _>(
                                        #key_expr,
                                        #functor
                                    )
                                }
                            }
                            Constructor::CompareEnum(compare_name) => {
                                let functor = gen_compare_enum_functor(compare_name);
                                quote! {
                                    turbo_tasks::macro_helpers::match_previous_node_by_type::<#ident, _>(
                                        #functor
                                    )
                                }
                            }
                            Constructor::KeyAndCompareEnum(key_expr, compare_name) => {
                                let functor = gen_compare_enum_functor(compare_name);
                                quote! {
                                    turbo_tasks::macro_helpers::match_previous_node_by_key::<#ident, _, _>(
                                        #key_expr,
                                        #functor
                                    )
                                }
                            }
                            Constructor::Key(_) => todo!(),
                        };
                        constructors.push(quote! {
                            #(#attrs)*
                            #vis #defaultness #sig {
                                let node = #get_node;
                                Self {
                                    node
                                }
                            }
                        });
                        i += 1;
                    }
                }
                _ => {}
            };
        }

        return quote! {
            impl #ref_ident {
                #(#constructors)*
            }
        };
    }

    fn generate_for_trait_impl(
        trait_ident: &Ident,
        struct_ident: &Ident,
        items: &Vec<ImplItem>,
    ) -> TokenStream2 {
        let register = get_register_trait_methods_ident(trait_ident, struct_ident);
        let ref_ident = get_ref_ident(struct_ident);
        let mut trait_registers = Vec::new();
        let mut impl_functions = Vec::new();
        for item in items.iter() {
            match item {
                ImplItem::Method(ImplItemMethod {
                    sig:
                        Signature {
                            ident,
                            inputs,
                            output,
                            ..
                        },
                    ..
                }) => {
                    let output_type = get_return_type(output);
                    let function_ident = get_trait_impl_function_ident(struct_ident, ident);
                    trait_registers.push(quote! {
                        slot_value_type.register_trait_method(#trait_ident.__type(), stringify!(#ident).to_string(), &*#function_ident);
                    });
                    let name =
                        Literal::string(&(struct_ident.to_string() + "::" + &ident.to_string()));
                    let native_function_code = gen_native_function_code(
                        quote! { #name },
                        quote! { #trait_ident::#ident },
                        &function_ident,
                        inputs,
                        &output_type,
                        Some(&ref_ident),
                    );
                    impl_functions.push(quote! {
                        #native_function_code
                    })
                }
                _ => {}
            }
        }
        quote! {
            #[allow(non_snake_case)]
            fn #register(slot_value_type: &mut turbo_tasks::SlotValueType) {
                slot_value_type.register_trait(#trait_ident.__type());
                #(#trait_registers)*
            }

            #(#impl_functions)*
        }
    }

    let item = parse_macro_input!(input as ItemImpl);

    if let Type::Path(TypePath {
        qself: None,
        path: Path { segments, .. },
    }) = &*item.self_ty
    {
        if segments.len() == 1 {
            if let Some(PathSegment {
                arguments: PathArguments::None,
                ident,
            }) = segments.first()
            {
                match &item.trait_ {
                    None => {
                        let code = generate_for_self_impl(ident, &item.items);
                        return quote! {
                            #item

                            #code
                        }
                        .into();
                    }
                    Some((_, Path { segments, .. }, _)) => {
                        if segments.len() == 1 {
                            if let Some(PathSegment {
                                arguments: PathArguments::None,
                                ident: trait_ident,
                            }) = segments.first()
                            {
                                let code = generate_for_trait_impl(trait_ident, ident, &item.items);
                                return quote! {
                                    #item

                                    #code
                                }
                                .into();
                            }
                        }
                    }
                }
            }
        }
    }
    item.span().unwrap().error("unsupported syntax").emit();
    quote! {
        #item
    }
    .into()
}

fn get_return_type(output: &ReturnType) -> Type {
    match output {
        ReturnType::Default => Type::Tuple(TypeTuple {
            paren_token: Paren::default(),
            elems: Punctuated::new(),
        }),
        ReturnType::Type(_, ref output_type) => (**output_type).clone(),
    }
}

#[proc_macro_attribute]
pub fn function(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemFn);
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = &item;
    let output_type = get_return_type(&sig.output);
    let ident = &sig.ident;
    let function_ident = get_function_ident(ident);
    let inline_ident = get_internal_function_ident(ident);

    let mut inline_sig = sig.clone();
    inline_sig.ident = inline_ident.clone();

    let mut external_sig = sig.clone();
    external_sig.asyncness = None;
    external_sig.output = parse_quote! { -> impl std::future::Future<Output = #output_type> };

    let mut input_extraction = Vec::new();
    let mut input_verification = Vec::new();
    let mut input_clone = Vec::new();
    let mut input_from_node = Vec::new();
    let mut input_arguments = Vec::new();
    let mut input_node_arguments = Vec::new();

    let mut index: i32 = 1;

    for input in sig.inputs.iter() {
        match input {
            FnArg::Receiver(_) => {
                item.span()
                    .unwrap()
                    .error("functions referencing self are not supported yet")
                    .emit();
            }
            FnArg::Typed(PatType { pat, ty, .. }) => {
                input_extraction.push(quote! {
                        let #pat = __iter
                            .next()
                            .ok_or_else(|| anyhow::anyhow!(concat!(stringify!(#ident), "() argument ", stringify!(#index), " (", stringify!(#pat), ") missing")))?;
                    });
                input_verification.push(quote! {
                        anyhow::Context::context(#ty::verify(&#pat), concat!(stringify!(#ident), "() argument ", stringify!(#index), " (", stringify!(#pat), ") invalid"))?;
                    });
                input_clone.push(quote! {
                    let #pat = std::clone::Clone::clone(&#pat);
                });
                input_from_node.push(quote! {
                    let #pat = #ty::from_slot_ref(#pat);
                });
                input_arguments.push(quote! {
                    #pat
                });
                input_node_arguments.push(quote! {
                    #pat.into()
                });
                index += 1;
            }
        }
    }

    let native_function_code = gen_native_function_code(
        quote! { stringify!(#ident) },
        quote! { #inline_ident },
        &function_ident,
        &sig.inputs,
        &output_type,
        None,
    );

    let convert_result_code = if is_empty_type(&output_type) {
        quote! {}
    } else {
        quote! { #output_type::from_slot_ref(result) }
    };

    return quote! {
        #(#attrs)*
        #vis #external_sig {
            let result = turbo_tasks::dynamic_call(&#function_ident, vec![#(#input_node_arguments),*]).unwrap();
            async { #convert_result_code }
        }

        #(#attrs)*
        #vis #inline_sig #block

        #native_function_code
    }
    .into();
}

fn is_empty_type(ty: &Type) -> bool {
    if let Type::Tuple(TypeTuple { elems, .. }) = ty {
        if elems.is_empty() {
            return true;
        }
    }
    false
}

fn gen_native_function_code(
    name_code: TokenStream2,
    original_function: TokenStream2,
    function_ident: &Ident,
    inputs: &Punctuated<FnArg, Token![,]>,
    output_type: &Type,
    self_ref_type: Option<&Ident>,
) -> TokenStream2 {
    let mut task_argument_options = Vec::new();
    let mut input_extraction = Vec::new();
    let mut input_verification = Vec::new();
    let mut input_clone = Vec::new();
    let mut input_from_node = Vec::new();
    let mut input_arguments = Vec::new();

    let mut index: i32 = 1;

    for input in inputs {
        match input {
            FnArg::Receiver(Receiver { mutability, .. }) => {
                if mutability.is_some() {
                    input.span().unwrap().error("mutable self is not supported in turbo_task traits (nodes are immutable)").emit();
                }
                let self_ref_type = self_ref_type.unwrap();
                task_argument_options.push(quote! {
                    #self_ref_type::get_default_task_argument_options()
                });
                input_extraction.push(quote! {
                    let __self = __iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!(concat!(#name_code, "() self argument missing")))?;
                });
                input_verification.push(quote! {
                    // anyhow::Context::context(#self_ref_type::verify(&__self), concat!(#name_code, "() self argument invalid"))?;
                });
                input_clone.push(quote! {
                    let __self = std::clone::Clone::clone(&__self);
                });
                input_from_node.push(quote! {
                    let __self = #self_ref_type::from_slot_ref(__self).await;
                });
                input_arguments.push(quote! {
                    &*__self
                });
            }
            FnArg::Typed(PatType { pat, ty, .. }) => {
                task_argument_options.push(quote! {
                    #ty::get_default_task_argument_options()
                });
                input_extraction.push(quote! {
                    let #pat = __iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!(concat!(#name_code, "() argument ", stringify!(#index), " (", stringify!(#pat), ") missing")))?;
                });
                input_verification.push(quote! {
                    // anyhow::Context::context(#ty::verify(&#pat), concat!(#name_code, "() argument ", stringify!(#index), " (", stringify!(#pat), ") invalid"))?;
                });
                input_clone.push(quote! {
                    let #pat = std::clone::Clone::clone(&#pat);
                });
                input_from_node.push(quote! {
                    let #pat = #ty::from_slot_ref(#pat);
                });
                input_arguments.push(quote! {
                    #pat
                });
                index += 1;
            }
        }
    }
    let original_call_code = if is_empty_type(output_type) {
        quote! {
            #original_function(#(#input_arguments),*).await;
            turbo_tasks::SlotRef::Nothing
        }
    } else {
        quote! { #original_function(#(#input_arguments),*).await.into() }
    };
    quote! {
        lazy_static::lazy_static! {
            static ref #function_ident: turbo_tasks::NativeFunction = turbo_tasks::NativeFunction::new(#name_code.to_string(), vec![#(#task_argument_options),*], |inputs| {
                let mut __iter = inputs.into_iter();
                #(#input_extraction)*
                if __iter.next().is_some() {
                    return Err(anyhow::anyhow!(concat!(#name_code, "() called with too many arguments")));
                }
                #(#input_verification)*
                Ok(Box::new(move || {
                    #(#input_clone)*
                    Box::pin(async move {
                        #(#input_from_node)*
                        #original_call_code
                    })
                }))
            });
        }
    }
}

#[proc_macro_attribute]
pub fn constructor(_args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

#[proc_macro_derive(TraceSlotRefs, attributes(trace_ignore))]
pub fn derive_trace_node_refs_attr(input: TokenStream) -> TokenStream {
    fn ignore_field(field: &Field) -> bool {
        !field
            .attrs
            .iter()
            .any(|attr| attr.path.is_ident("trace_ignore"))
    }

    let item = parse_macro_input!(input as Item);

    let (ident, trace_items) = match &item {
        Item::Enum(ItemEnum {
            ident, variants, ..
        }) => (ident, {
            let variants_code: Vec<_> = variants.iter().map(|variant| {
                let variant_ident = &variant.ident;
                match &variant.fields {
                    Fields::Named(FieldsNamed{ named, ..}) => {
                        let idents: Vec<_> = named.iter()
                            .filter(|field| ignore_field(field))
                            .filter_map(|field| field.ident.clone())
                            .collect();
                        quote! {
                            #ident::#variant_ident{ #(ref #idents),* } => {
                                #(
                                    turbo_tasks::trace::TraceSlotRefs::trace_node_refs(#idents, context);
                                )*
                            }
                        }
                    },
                    Fields::Unnamed(FieldsUnnamed{ unnamed, .. }) => {
                        let idents: Vec<_> = unnamed.iter()
                            .enumerate()
                            .filter(|(_, field)| ignore_field(field))
                            .map(|(i, field)| Ident::new(&format!("tuple_item_{}", i), field.span()))
                            .collect();
                        quote! {
                            #ident::#variant_ident( #(ref #idents),* ) => {
                                #(
                                    turbo_tasks::trace::TraceSlotRefs::trace_node_refs(#idents, context);
                                )*
                            }
                        }
                    },
                    Fields::Unit => quote! {
                        #ident::#variant_ident => {}
                    },
                }
            }).collect();
            quote! {
                match self {
                    #(#variants_code)*
                }
            }
        }),
        Item::Struct(ItemStruct { ident, fields, .. }) => (
            ident,
            match fields {
                Fields::Named(FieldsNamed { named, .. }) => {
                    let idents: Vec<_> = named
                        .iter()
                        .filter(|field| ignore_field(field))
                        .filter_map(|field| field.ident.clone())
                        .collect();
                    quote! {
                        #(
                            turbo_tasks::trace::TraceSlotRefs::trace_node_refs(&self.#idents, context);
                        )*
                    }
                }
                Fields::Unnamed(FieldsUnnamed { unnamed, .. }) => {
                    let indicies: Vec<_> = unnamed
                        .iter()
                        .enumerate()
                        .filter(|(_, field)| ignore_field(field))
                        .map(|(i, _)| Literal::usize_unsuffixed(i))
                        .collect();
                    quote! {
                        #(
                            turbo_tasks::trace::TraceSlotRefs::trace_node_refs(&self.#indicies, context);
                        )*
                    }
                }
                Fields::Unit => quote! {},
            },
        ),
        _ => {
            item.span().unwrap().error("unsupported syntax").emit();

            return quote! {}.into();
        }
    };
    quote! {
        impl turbo_tasks::trace::TraceSlotRefs for #ident {
            fn trace_node_refs(&self, context: &mut turbo_tasks::trace::TraceSlotRefsContext) {
                #trace_items
            }
        }
    }
    .into()
}
