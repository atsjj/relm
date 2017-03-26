/*
 * Copyright (c) 2017 Boucher, Antoni <bouanto@zoho.com>
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software is furnished to do so,
 * subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
 * FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
 * COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
 * IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

/*
 * TODO: allow pattern matching by creating a function update(&mut self, Quit: Msg, model: &mut Model) so that we can separate
 * the update function in multiple functions.
 * TODO: provide default parameter for constructor (like Toplevel). Is it still necessary?
 * Perhaps for construct-only properties (if they don't have a default value, does this happen?).
 * TODO: think about conditions and loops (widget-list).
 */

#![feature(proc_macro)]

#[macro_use]
extern crate lazy_static;
extern crate proc_macro;
#[macro_use]
extern crate quote;
extern crate syn;

mod adder;
mod gen;
mod parser;
mod walker;

use std::collections::{HashMap, HashSet};

use proc_macro::TokenStream;
use quote::{Tokens, ToTokens};
use syn::{Delimited, FunctionRetTy, Ident, ImplItem, Mac, Path, TokenTree, parse_expr, parse_item};
use syn::fold::Folder;
use syn::ItemKind::Impl;
use syn::ImplItemKind::{Const, Macro, Method, Type};
use syn::Ty;
use syn::visit::Visitor;

use adder::{Adder, Property};
use gen::gen;
use parser::{Widget, parse};
use walker::ModelVariableVisitor;

type PropertyModelMap = HashMap<Ident, HashSet<Property>>;

#[proc_macro_attribute]
pub fn widget(_attributes: TokenStream, input: TokenStream) -> TokenStream {
    let source = input.to_string();
    let mut ast = parse_item(&source).unwrap();
    if let Impl(unsafety, polarity, generics, path, typ, items) = ast.node {
        let name = get_name(&typ);
        let mut new_items = vec![];
        let mut container_method = None;
        let mut root_widget = None;
        let mut root_widget_type = None;
        let mut widget_model_type = None;
        let mut update_method = None;
        let mut properties_model_map = None;
        let mut container_type = None;
        let mut model_type = None;
        for item in items {
            let i = item.clone();
            let new_item =
                match item.node {
                    Const(_, _) => panic!("Unexpected const item"),
                    Macro(mac) => {
                        let (view, map) = get_view(mac, &name, &mut root_widget, &mut root_widget_type);
                        properties_model_map = Some(map);
                        view
                    },
                    Method(sig, _) => {
                        match item.ident.to_string().as_ref() {
                            "container" => {
                                container_method = Some(i);
                                continue;
                            },
                            "model" => {
                                if let FunctionRetTy::Ty(Ty::Path(_, path)) = sig.decl.output {
                                    widget_model_type = Some(path);
                                }
                                i
                            },
                            "subscriptions" => i,
                            "update" => {
                                update_method = Some(i);
                                continue
                            },
                            "update_command" => i, // TODO: automatically create this function from the events present in the view (or by splitting the update() fucntion).
                            method_name => panic!("Unexpected method {}", method_name),
                        }
                    },
                    Type(_) => {
                        if item.ident == Ident::new("Container") {
                            container_type = Some(i);
                        }
                        else if item.ident == Ident::new("Model") {
                            model_type = Some(i);
                        }
                        else {
                            panic!("Unexpected type item {:?}", item.ident);
                        }
                        continue;
                    },
                };
            new_items.push(new_item);
        }
        new_items.push(get_model_type(model_type, widget_model_type));
        new_items.push(get_container_type(container_type, root_widget_type));
        new_items.push(get_update(update_method.unwrap(), properties_model_map.unwrap()));
        new_items.push(get_container(container_method, root_widget));
        let item = Impl(unsafety, polarity, generics, path, typ, new_items);
        ast.node = item;
        let expanded = quote! {
            #ast
        };
        expanded.parse().unwrap()
    }
    else {
        panic!("Expected impl");
    }
}

fn block_to_impl_item(tokens: Tokens) -> ImplItem {
    let implementation = quote! {
        impl Test {
            #tokens
        }
    };
    let implementation = parse_item(implementation.as_str()).unwrap();
    match implementation.node {
        Impl(_, _, _, _, _, items) => items[0].clone(),
        _ => unreachable!(),
    }
}

fn impl_view(name: &Ident, tokens: Vec<TokenTree>, root_widget: &mut Option<Ident>, root_widget_type: &mut Option<Ident>) -> (ImplItem, PropertyModelMap) {
    if let TokenTree::Delimited(Delimited { ref tts, .. }) = tokens[0] {
        let widget = parse(tts);
        let mut properties_model_map = HashMap::new();
        get_properties_model_map(&widget, &mut properties_model_map);
        let view = gen(name, widget, root_widget, root_widget_type);
        let item = block_to_impl_item(quote! {
            #[allow(unused_variables)]
            fn view(relm: RemoteRelm<Msg>, model: &Self::Model) -> Self {
                #view
            }
        });
        (item, properties_model_map)
    }
    else {
        panic!("Expected `{{` but found `{:?}` in view! macro", tokens[0]);
    }
}

fn get_container(container_method: Option<ImplItem>, root_widget: Option<Ident>) -> ImplItem {
    container_method.unwrap_or_else(|| {
        let root_widget = root_widget.unwrap();
        block_to_impl_item(quote! {
            fn container(&self) -> &Self::Container {
                &self.#root_widget
            }
        })
    })
}

fn get_container_type(container_type: Option<ImplItem>, root_widget_type: Option<Ident>) -> ImplItem {
    container_type.unwrap_or_else(|| {
        let root_widget_type = root_widget_type.unwrap();
        block_to_impl_item(quote! {
            type Container = #root_widget_type;
        })
    })
}

fn get_model_type(model_type: Option<ImplItem>, widget_model_type: Option<Path>) -> ImplItem {
    model_type.unwrap_or_else(|| {
        let widget_model_type = widget_model_type.unwrap();
        block_to_impl_item(quote! {
            type Model = #widget_model_type;
        })
    })
}

fn get_name(typ: &Ty) -> Ident {
    if let Ty::Path(_, ref path) = *typ {
        let mut tokens = Tokens::new();
        path.to_tokens(&mut tokens);
        Ident::new(tokens.to_string())
    }
    else {
        panic!("Expected Path")
    }
}

/*
 * TODO: Create a control flow graph for each variable of the model.
 * Add the set_property() calls in every leaf of every graphs.
 */
fn get_update(mut func: ImplItem, map: PropertyModelMap) -> ImplItem {
    if let Method(_, ref mut block) = func.node {
        let mut adder = Adder::new(&map);
        *block = adder.fold_block(block.clone());
    }
    // TODO: consider gtk::main_quit() as return.
    func
}

fn get_view(mac: Mac, name: &Ident, root_widget: &mut Option<Ident>, root_widget_type: &mut Option<Ident>) -> (ImplItem, PropertyModelMap) {
    let segments = mac.path.segments;
    if segments.len() == 1 && segments[0].ident.to_string() == "view" {
        impl_view(name, mac.tts, root_widget, root_widget_type)
    }
    else {
        panic!("Unexpected macro item")
    }
}

/*
 * The map maps model variable name to a vector of tuples (widget name, property name).
 */
fn get_properties_model_map(widget: &Widget, map: &mut PropertyModelMap) {
    for (name, value) in &widget.properties {
        let string = value.parse::<String>().unwrap();
        let expr = parse_expr(&string).unwrap();
        let mut visitor = ModelVariableVisitor::new();
        visitor.visit_expr(&expr);
        let model_variables = visitor.idents;
        for var in model_variables {
            let set = map.entry(var).or_insert_with(HashSet::new);
            set.insert(Property {
                expr: string.clone(),
                name: name.clone(),
                widget_name: widget.name.clone(),
            });
        }
    }
    for child in &widget.children {
        get_properties_model_map(child, map);
    }
}
