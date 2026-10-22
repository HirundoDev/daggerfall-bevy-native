//! `commerce` — **inventory-lite** trade (M5, QW3): gold + goods list with a
//! dialogue/trade session.
//!
//! **Documented minimal scope (C3 pack M5):** in-session memory state,
//! **0 persistence** (unless specced) — no files, no Event Store.
//! **Price-agnostic:** `buy`/`sell` receive a `price` already computed by the
//! caller — this organ does NOT depend on `daggerfall-skills` (organs never
//! call each other; `app` composes: DFU formula + session).
//!
//! Organ rules 📦 Inventory (bevy-bridge-matrix): resource pools +
//! item collections · 0 Bevy · 0 winit · 0 persistence.

/// Merchant catalog item (declared synthetic content — the M5 note documents
/// the delta against DFU tables).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockItem {
    /// Stable id (buys/sells reference it).
    pub id: String,
    /// Visible name (dialogue/menu).
    pub name: String,
    /// Base cost for the price formula (DFU `cost` input).
    pub base_cost: i32,
    /// Units in stock.
    pub stock: i32,
}

/// Merchant catalog: name, shop quality (DFU `shopQuality`) and offered
/// inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerchantStock {
    /// Merchant NPC name (dialogue).
    pub merchant_name: String,
    /// DFU shop quality (0-20) — parameter of the price formula.
    pub shop_quality: i32,
    /// Offered items.
    pub items: Vec<StockItem>,
}

impl MerchantStock {
    /// Looks an item up by id.
    pub fn item(&self, id: &str) -> Option<&StockItem> {
        self.items.iter().find(|item| item.id == id)
    }
}

/// Player inventory-lite: **gold** (integer — NFR NUMERIC-never-FLOAT)
/// + goods list `(id, quantity)`. No persistence (QW3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InventoryLite {
    /// Gold coins.
    pub gold: i32,
    /// Goods `(id, quantity)` — 0 duplicates per id.
    pub goods: Vec<(String, i32)>,
}

impl InventoryLite {
    /// Inventory with `gold` and 0 goods.
    pub fn new(gold: i32) -> Self {
        Self {
            gold,
            goods: Vec::new(),
        }
    }

    /// Quantity of good `id` (0 if not held).
    pub fn count(&self, id: &str) -> i32 {
        self.goods
            .iter()
            .find(|(good_id, _)| good_id == id)
            .map(|(_, qty)| *qty)
            .unwrap_or(0)
    }

    fn add(&mut self, id: &str, qty: i32) {
        if let Some(entry) = self.goods.iter_mut().find(|(good_id, _)| good_id == id) {
            entry.1 += qty;
        } else {
            self.goods.push((id.to_string(), qty));
        }
    }

    fn remove(&mut self, id: &str, qty: i32) {
        if let Some(position) = self
            .goods
            .iter()
            .position(|(good_id, _)| good_id == id)
        {
            self.goods[position].1 -= qty;
            if self.goods[position].1 <= 0 {
                self.goods.remove(position);
            }
        }
    }
}

/// Trade transaction error (state NEVER mutates on `Err`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeError {
    /// The dialogue/menu is not open (interact with the E key first).
    DialogueClosed,
    /// Not enough gold for `price`.
    NotEnoughGold,
    /// The item is not in stock (or stock 0).
    OutOfStock,
    /// The player does not own the good (sell).
    NotOwned,
    /// `price < 0` — invalid price.
    InvalidPrice,
}

/// Trade session: catalog + inventory + dialogue/menu state.
#[derive(Debug, Clone, PartialEq)]
pub struct TradeSession {
    /// Merchant catalog.
    pub merchant: MerchantStock,
    /// Player inventory-lite.
    pub inventory: InventoryLite,
    /// `true` when the interaction (E) opened the dialogue/menu.
    pub dialogue_open: bool,
}

impl TradeSession {
    /// Session with the dialogue closed.
    pub fn new(merchant: MerchantStock, inventory: InventoryLite) -> Self {
        Self {
            merchant,
            inventory,
            dialogue_open: false,
        }
    }

    /// Opens the dialogue (done by the raycast+E interaction, not by this
    /// method directly during gameplay).
    pub fn open_dialogue(&mut self) {
        self.dialogue_open = true;
    }

    /// Closes the dialogue/menu.
    pub fn close_dialogue(&mut self) {
        self.dialogue_open = false;
    }

    /// Minimal NPC greeting (dialogue).
    pub fn greeting(&self) -> String {
        format!(
            "Greetings, traveler. — {}, shop of quality {}",
            self.merchant.merchant_name, self.merchant.shop_quality
        )
    }

    /// Buy/sell menu line with gold (VE evidence / title bar).
    pub fn menu_line(&self) -> String {
        let buy = self
            .merchant
            .items
            .first()
            .map(|item| item.name.as_str())
            .unwrap_or("out of stock");
        let sell = self
            .inventory
            .goods
            .first()
            .map(|(id, qty)| format!("{id}×{qty}"))
            .unwrap_or("nothing to sell".to_string());
        format!(
            "gold={} · [B] buy {buy} · [V] sell {sell}",
            self.inventory.gold
        )
    }

    /// First item of the stock (deterministic menu selection).
    pub fn first_stock(&self) -> Option<&StockItem> {
        self.merchant.items.first()
    }

    /// First owned good (deterministic menu selection).
    pub fn first_good(&self) -> Option<&(String, i32)> {
        self.inventory.goods.first()
    }

    /// Buys `item_id` paying `price` (computed by the caller with its own
    /// organ's formula). Atomic transition: on `Err` 0 mutation; on `Ok` gold
    /// debited + stock−1 + good +1.
    pub fn buy(&mut self, item_id: &str, price: i32) -> Result<i32, TradeError> {
        if !self.dialogue_open {
            return Err(TradeError::DialogueClosed);
        }
        if price < 0 {
            return Err(TradeError::InvalidPrice);
        }
        let stock = self
            .merchant
            .items
            .iter()
            .find(|item| item.id == item_id)
            .map(|item| item.stock)
            .ok_or(TradeError::OutOfStock)?;
        if stock <= 0 {
            return Err(TradeError::OutOfStock);
        }
        if self.inventory.gold < price {
            return Err(TradeError::NotEnoughGold);
        }
        // Single transition (gold−=price, stock−=1, good+=1).
        self.inventory.gold -= price;
        let item = self
            .merchant
            .items
            .iter_mut()
            .find(|item| item.id == item_id)
            .expect("stock located in the precheck");
        item.stock -= 1;
        self.inventory.add(item_id, 1);
        Ok(price)
    }

    /// Sells `good_id` to the merchant charging `price`. Atomic transition:
    /// on `Err` 0 mutation; on `Ok` gold credited + good−1.
    pub fn sell(&mut self, good_id: &str, price: i32) -> Result<i32, TradeError> {
        if !self.dialogue_open {
            return Err(TradeError::DialogueClosed);
        }
        if price < 0 {
            return Err(TradeError::InvalidPrice);
        }
        if self.inventory.count(good_id) <= 0 {
            return Err(TradeError::NotOwned);
        }
        self.inventory.gold += price;
        self.inventory.remove(good_id, 1);
        Ok(price)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(gold: i32) -> TradeSession {
        TradeSession::new(
            MerchantStock {
                merchant_name: "Vendel".into(),
                shop_quality: 10,
                items: vec![StockItem {
                    id: "potion".into(),
                    name: "minor potion".into(),
                    base_cost: 80,
                    stock: 5,
                }],
            },
            {
                let mut inventory = InventoryLite::new(gold);
                inventory.add("furs", 1);
                inventory
            },
        )
    }

    #[test]
    fn buy_requires_open_dialogue() {
        let mut session = session(100);
        assert_eq!(session.buy("potion", 45), Err(TradeError::DialogueClosed));
        assert_eq!(session.inventory.gold, 100, "0 mutation on Err");
        session.open_dialogue();
        assert!(session.dialogue_open);
        session.close_dialogue();
        assert!(!session.dialogue_open);
    }

    #[test]
    fn buy_deducts_gold_and_adds_good() {
        let mut session = session(100);
        session.open_dialogue();
        assert_eq!(session.buy("potion", 45), Ok(45));
        assert_eq!(session.inventory.gold, 55);
        assert_eq!(session.inventory.count("potion"), 1);
        assert_eq!(session.merchant.item("potion").unwrap().stock, 4);
    }

    #[test]
    fn buy_insufficient_gold_rejected_state_unchanged() {
        let mut session = session(5);
        session.open_dialogue();
        assert_eq!(session.buy("potion", 45), Err(TradeError::NotEnoughGold));
        assert_eq!(session.inventory.gold, 5, "gold untouched");
        assert_eq!(session.inventory.count("potion"), 0, "good untouched");
    }

    #[test]
    fn buy_unknown_or_exhausted_stock_rejected() {
        let mut session = session(100);
        session.open_dialogue();
        assert_eq!(session.buy("sword", 10), Err(TradeError::OutOfStock));
        assert_eq!(session.inventory.gold, 100);
    }

    #[test]
    fn sell_credits_gold_and_removes_good() {
        let mut session = session(100);
        session.open_dialogue();
        assert_eq!(session.sell("furs", 25), Ok(25));
        assert_eq!(session.inventory.gold, 125);
        assert_eq!(session.inventory.count("furs"), 0);
        assert_eq!(session.sell("furs", 25), Err(TradeError::NotOwned));
        assert_eq!(session.inventory.gold, 125, "0 mutation on Err");
    }

    #[test]
    fn menu_and_greeting_are_stable() {
        let mut session = session(100);
        let menu = session.menu_line();
        assert!(menu.contains("buy") && menu.contains("sell") && menu.contains("gold=100"));
        assert!(session.greeting().contains("Vendel"));
        session.open_dialogue();
        assert!(session.dialogue_open);
    }
}
