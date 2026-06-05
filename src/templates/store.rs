// ── Store template file contents ─────────────────────────────────────────────
//
// A minimal e-commerce UI with a product grid, cart sidebar, and header.
// All components use Dioxus 0.7.9 APIs (use_signal, Element, rsx!).

pub const CARGO_TOML: &str = r#"[package]
name = "store"
version = "0.1.0"
edition = "2021"

[dependencies]
dioxus = { version = "0.7.9", features = ["desktop"] }
"#;

pub const MAIN_RS: &str = r#"use dioxus::prelude::*;

mod state;
mod components;

use state::AppState;
use components::{Header, ProductGrid, CartSidebar};

fn main() {
    dioxus::LaunchBuilder::desktop().launch(App);
}

#[component]
fn App() -> Element {
    let state = use_signal(AppState::default);

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100vh; font-family: sans-serif;",
            Header { state }
            div {
                style: "display: flex; flex: 1; overflow: hidden;",
                ProductGrid { state }
                CartSidebar { state }
            }
        }
    }
}
"#;

pub const STATE_RS: &str = r#"use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Product {
    pub id: usize,
    pub name: &'static str,
    pub price: f64,
    pub image: &'static str,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppState {
    pub products: Vec<Product>,
    pub cart: Vec<(Product, usize)>,
    pub cart_open: bool,
}

impl AppState {
    pub fn add_to_cart(&mut self, product: &Product) {
        if let Some((_, qty)) = self.cart.iter_mut().find(|(p, _)| p.id == product.id) {
            *qty += 1;
        } else {
            self.cart.push((product.clone(), 1));
        }
    }

    pub fn remove_from_cart(&mut self, product_id: usize) {
        self.cart.retain(|(p, _)| p.id != product_id);
    }

    pub fn cart_total(&self) -> f64 {
        self.cart.iter().map(|(p, qty)| p.price * *qty as f64).sum()
    }

    pub fn cart_count(&self) -> usize {
        self.cart.iter().map(|(_, qty)| qty).sum()
    }
}
"#;

pub const COMPONENTS_MOD_RS: &str = r#"pub mod product_grid;
pub mod cart;
pub mod header;

pub use product_grid::ProductGrid;
pub use cart::CartSidebar;
pub use header::Header;
"#;

pub const PRODUCT_GRID_RS: &str = r#"use dioxus::prelude::*;
use crate::state::{AppState, Product};

#[component]
pub fn ProductGrid(state: Signal<AppState>) -> Element {
    let products = state.read().products.clone();

    rsx! {
        div {
            style: "flex: 1; padding: 24px; overflow-y: auto; background: #f8f9fa;",
            div {
                style: "display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 24px;",
                for product in products {
                    ProductCard { state, product: product.clone() }
                }
            }
        }
    }
}

#[component]
fn ProductCard(state: Signal<AppState>, product: Product) -> Element {
    rsx! {
        div {
            style: "background: white; border-radius: 12px; padding: 16px; box-shadow: 0 2px 8px rgba(0,0,0,0.06); transition: transform 0.2s;",
            onmouseenter: move |_| {},
            onmouseleave: move |_| {},

            div {
                style: "width: 100%; height: 160px; background: #e9ecef; border-radius: 8px; display: flex; align-items: center; justify-content: center; color: #adb5bd; font-size: 14px; margin-bottom: 12px;",
                "{product.image}"
            }

            div {
                style: "font-weight: 600; font-size: 15px; margin-bottom: 4px; color: #212529;",
                "{product.name}"
            }

            div {
                style: "color: #f97316; font-weight: 700; font-size: 16px; margin-bottom: 12px;",
                "${product.price:.2}"
            }

            button {
                style: "width: 100%; padding: 10px; background: #f97316; color: white; border: none; border-radius: 6px; font-weight: 600; cursor: pointer; font-size: 14px;",
                onclick: move |_| {
                    state.write().add_to_cart(&product);
                },
                "Add to cart"
            }
        }
    }
}
"#;

pub const CART_RS: &str = r#"use dioxus::prelude::*;
use crate::state::AppState;

#[component]
pub fn CartSidebar(state: Signal<AppState>) -> Element {
    let count = state.read().cart_count();
    let total = state.read().cart_total();
    let cart_items = state.read().cart.clone();
    let is_empty = cart_items.is_empty();

    rsx! {
        div {
            style: "width: 320px; background: white; border-left: 1px solid #e9ecef; display: flex; flex-direction: column;",

            div {
                style: "padding: 16px 20px; border-bottom: 1px solid #e9ecef; display: flex; align-items: center; justify-content: space-between;",
                div {
                    style: "font-weight: 700; font-size: 16px; color: #212529;",
                    "Shopping Cart"
                }
                if count > 0 {
                    div {
                        style: "background: #f97316; color: white; font-size: 12px; font-weight: 700; padding: 2px 8px; border-radius: 10px;",
                        "{count}"
                    }
                }
            }

            div {
                style: "flex: 1; overflow-y: auto; padding: 16px;",
                if is_empty {
                    div {
                        style: "color: #adb5bd; text-align: center; margin-top: 40px; font-size: 14px;",
                        "Your cart is empty"
                    }
                } else {
                    for (product, qty) in cart_items {
                        div {
                            key: "{product.id}",
                            style: "display: flex; gap: 12px; margin-bottom: 16px; align-items: center;",

                            div {
                                style: "width: 48px; height: 48px; background: #e9ecef; border-radius: 6px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; font-size: 10px; color: #adb5bd;",
                                "img"
                            }

                            div {
                                style: "flex: 1; min-width: 0;",
                                div {
                                    style: "font-size: 14px; font-weight: 600; color: #212529; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                    "{product.name}"
                                }
                                div {
                                    style: "font-size: 13px; color: #6c757d;",
                                    "Qty: {qty} × ${product.price:.2}"
                                }
                            }

                            button {
                                style: "background: transparent; border: none; color: #dc3545; font-size: 12px; cursor: pointer; padding: 4px;",
                                onclick: move |_| {
                                    state.write().remove_from_cart(product.id);
                                },
                                "Remove"
                            }
                        }
                    }
                }
            }

            if !is_empty {
                div {
                    style: "padding: 16px 20px; border-top: 1px solid #e9ecef;",
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;",
                        span { style: "color: #6c757d; font-size: 14px;", "Total" }
                        span { style: "font-weight: 700; font-size: 18px; color: #212529;", "${total:.2}" }
                    }
                    button {
                        style: "width: 100%; padding: 12px; background: #212529; color: white; border: none; border-radius: 6px; font-weight: 700; cursor: pointer; font-size: 14px;",
                        "Checkout"
                    }
                }
            }
        }
    }
}
"#;

pub const HEADER_RS: &str = r#"use dioxus::prelude::*;
use crate::state::AppState;

#[component]
pub fn Header(state: Signal<AppState>) -> Element {
    let count = state.read().cart_count();

    rsx! {
        div {
            style: "height: 56px; background: #212529; color: white; display: flex; align-items: center; justify-content: space-between; padding: 0 24px; flex-shrink: 0;",

            div {
                style: "font-weight: 700; font-size: 18px; letter-spacing: -0.5px;",
                "🛍️ Store"
            }

            div {
                style: "display: flex; align-items: center; gap: 16px;",

                div {
                    style: "position: relative; cursor: pointer;",
                    onclick: move |_| {
                        let open = state.read().cart_open;
                        state.write().cart_open = !open;
                    },

                    svg {
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "white",
                        stroke_width: "2",
                        path { d: "M6 2L3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4z" }
                        line { x1: "3", y1: "6", x2: "21", y2: "6" }
                        path { d: "M16 10a4 4 0 0 1-8 0" }
                    }

                    if count > 0 {
                        div {
                            style: "position: absolute; top: -6px; right: -6px; background: #f97316; color: white; font-size: 10px; font-weight: 700; width: 18px; height: 18px; border-radius: 50%; display: flex; align-items: center; justify-content: center;",
                            "{count}"
                        }
                    }
                }
            }
        }
    }
}
"#;
