use crate::models::session::{InventoryItem, SessionSnapshot};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryOperation {
    pub action: String,
    pub item_id: String,
    pub item_name: String,
    pub category: String,
    pub quantity_delta: i32,
    pub reason: String,
    pub description: String,
    pub tags: Vec<String>,
    pub owner_type: String,
    pub owner_id: String,
    pub visibility: String,
    pub disclosed_to_add: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InventoryRuntimeResult {
    pub inventory_items: Vec<InventoryItem>,
    pub operations: Vec<InventoryOperation>,
    pub system_messages: Vec<String>,
    pub debug_lines: Vec<String>,
}

pub struct InventoryService;

impl InventoryService {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate_turn(
        &self,
        session: &SessionSnapshot,
        player_input: &str,
        location_override: Option<&str>,
    ) -> InventoryRuntimeResult {
        let current_location = location_override.unwrap_or(&session.location);
        let mut working_inventory = session.inventory_items.clone();
        let mut operations = Vec::new();
        let mut system_messages = Vec::new();

        if let Some(operation) = self.resolve_pickup_operation(current_location, player_input) {
            working_inventory = self.apply_operation(working_inventory, &operation);
            system_messages.push(format!("库存运行时：拾取 {}", operation.item_name));
            operations.push(operation);
        }

        if let Some(operation) = self.resolve_transfer_operation(&working_inventory, player_input) {
            working_inventory = self.apply_operation(working_inventory, &operation);
            system_messages.push(format!("库存运行时：转交 {}", operation.item_name));
            operations.push(operation);
        } else if let Some(operation) = self.resolve_use_operation(
            &working_inventory,
            player_input,
            &session.visible_characters,
        ) {
            working_inventory = self.apply_operation(working_inventory, &operation);
            system_messages.push(format!("库存运行时：使用 {}", operation.item_name));
            operations.push(operation);
        }

        let debug_lines = operations
            .iter()
            .map(|operation| {
                format!(
                    "InventoryRuntime {} {} delta={}",
                    operation.action, operation.item_name, operation.quantity_delta
                )
            })
            .collect::<Vec<_>>();

        InventoryRuntimeResult {
            inventory_items: working_inventory,
            operations,
            system_messages,
            debug_lines,
        }
    }

    fn resolve_pickup_operation(
        &self,
        current_location: &str,
        player_input: &str,
    ) -> Option<InventoryOperation> {
        if !self.contains_any(
            player_input,
            &["观察", "搜索", "搜寻", "拾取", "捡起", "获得", "拿起"],
        ) {
            return None;
        }
        let item = self.resolve_pickup_item(current_location)?;
        let quantity = self.resolve_requested_quantity(player_input, item.quantity, 1);
        Some(InventoryOperation {
            action: "add".to_string(),
            item_id: item.item_id,
            item_name: item.name,
            category: item.category,
            quantity_delta: quantity,
            reason: format!("scene_pickup:{current_location}"),
            description: item.description,
            tags: item.tags,
            owner_type: item.owner_type,
            owner_id: item.owner_id,
            visibility: item.visibility,
            disclosed_to_add: Vec::new(),
        })
    }

    fn resolve_transfer_operation(
        &self,
        inventory: &[InventoryItem],
        player_input: &str,
    ) -> Option<InventoryOperation> {
        if !self.contains_any(
            player_input,
            &[
                "交给", "交出", "递给", "交换", "放下", "扔下", "丢弃", "上交",
            ],
        ) {
            return None;
        }
        let candidate = self.resolve_target_item(inventory, player_input)?;
        let quantity = self.resolve_requested_quantity(player_input, candidate.quantity, 1);
        Some(InventoryOperation {
            action: "transfer".to_string(),
            item_id: candidate.item_id,
            item_name: candidate.name,
            category: candidate.category,
            quantity_delta: -quantity,
            reason: "player_transfer".to_string(),
            description: candidate.description,
            tags: candidate.tags,
            owner_type: candidate.owner_type,
            owner_id: candidate.owner_id,
            visibility: candidate.visibility,
            disclosed_to_add: Vec::new(),
        })
    }

    fn resolve_use_operation(
        &self,
        inventory: &[InventoryItem],
        player_input: &str,
        visible_characters: &[String],
    ) -> Option<InventoryOperation> {
        if !self.contains_any(
            player_input,
            &[
                "使用", "出示", "打开", "启动", "服用", "查看", "告诉", "展示",
            ],
        ) {
            return None;
        }
        let candidate = self.resolve_target_item(inventory, player_input)?;
        let quantity = self.resolve_requested_quantity(player_input, candidate.quantity, 1);
        let quantity_delta = if self.should_consume_on_use(&candidate, player_input) {
            -quantity
        } else {
            0
        };
        let disclosed_to_add = if self.contains_any(player_input, &["告诉", "展示", "出示"]) {
            visible_characters
                .iter()
                .filter(|name| !name.trim().is_empty())
                .cloned()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        Some(InventoryOperation {
            action: "use".to_string(),
            item_id: candidate.item_id,
            item_name: candidate.name,
            category: candidate.category,
            quantity_delta,
            reason: if quantity_delta < 0 {
                "player_use_consumed".to_string()
            } else {
                "player_use_recorded".to_string()
            },
            description: candidate.description,
            tags: candidate.tags,
            owner_type: candidate.owner_type,
            owner_id: candidate.owner_id,
            visibility: candidate.visibility,
            disclosed_to_add,
        })
    }

    fn resolve_pickup_item(&self, current_location: &str) -> Option<InventoryItem> {
        let item = if current_location.contains("商场") {
            Some(InventoryItem {
                item_id: format!("item-{}", Uuid::new_v4().simple()),
                name: "通行证".to_string(),
                category: "key-item".to_string(),
                quantity: 1,
                description: "用于通过内部权限门的临时通行证。".to_string(),
                tags: vec!["access".to_string(), "scene:mall".to_string()],
                owner_type: "player".to_string(),
                owner_id: "player".to_string(),
                visibility: "private".to_string(),
                disclosed_to: Vec::new(),
            })
        } else if current_location.contains("避难") || current_location.contains("高架") {
            Some(InventoryItem {
                item_id: format!("item-{}", Uuid::new_v4().simple()),
                name: "潮湿图纸".to_string(),
                category: "clue".to_string(),
                quantity: 1,
                description: "记录局部断电线路的旧图纸。".to_string(),
                tags: vec!["clue".to_string(), "electricity".to_string()],
                owner_type: "player".to_string(),
                owner_id: "player".to_string(),
                visibility: "private".to_string(),
                disclosed_to: Vec::new(),
            })
        } else if current_location.contains("藏经阁") {
            Some(InventoryItem {
                item_id: format!("item-{}", Uuid::new_v4().simple()),
                name: "残页拓片".to_string(),
                category: "clue".to_string(),
                quantity: 1,
                description: "从藏经阁里找到的残页拓片。".to_string(),
                tags: vec!["clue".to_string(), "scripture".to_string()],
                owner_type: "player".to_string(),
                owner_id: "player".to_string(),
                visibility: "private".to_string(),
                disclosed_to: Vec::new(),
            })
        } else {
            None
        }?;
        Some(item)
    }

    fn resolve_target_item(
        &self,
        inventory: &[InventoryItem],
        player_input: &str,
    ) -> Option<InventoryItem> {
        inventory
            .iter()
            .cloned()
            .map(|item| (self.score_item_match(&item, player_input), item))
            .max_by(|left, right| left.0.cmp(&right.0))
            .and_then(|(score, item)| if score > 0 { Some(item) } else { None })
    }

    fn score_item_match(&self, item: &InventoryItem, player_input: &str) -> i32 {
        let mut score = 0;
        if player_input.contains(&item.name) {
            score += 10;
        }
        if item
            .tags
            .iter()
            .any(|tag| !tag.is_empty() && player_input.contains(tag))
        {
            score += 4;
        }
        if player_input.contains(&item.category) {
            score += 2;
        }
        if item
            .description
            .chars()
            .take(8)
            .any(|ch| player_input.contains(ch))
        {
            score += 1;
        }
        score
    }

    fn resolve_requested_quantity(
        &self,
        player_input: &str,
        max_quantity: i32,
        fallback: i32,
    ) -> i32 {
        let numbers = [
            ("一", 1),
            ("二", 2),
            ("两", 2),
            ("三", 3),
            ("四", 4),
            ("五", 5),
            ("六", 6),
            ("七", 7),
            ("八", 8),
            ("九", 9),
        ];
        for (needle, value) in numbers {
            if player_input.contains(needle) {
                return value.min(max_quantity.max(1));
            }
        }
        fallback.min(max_quantity.max(1))
    }

    fn should_consume_on_use(&self, item: &InventoryItem, player_input: &str) -> bool {
        let consuming_categories = ["clue", "consumable", "material"];
        if consuming_categories.contains(&item.category.as_str()) {
            return true;
        }
        self.contains_any(player_input, &["拆", "消耗", "吃", "喝", "用掉"])
    }

    fn contains_any(&self, text: &str, needles: &[&str]) -> bool {
        needles.iter().any(|needle| text.contains(needle))
    }

    fn apply_operation(
        &self,
        mut inventory: Vec<InventoryItem>,
        operation: &InventoryOperation,
    ) -> Vec<InventoryItem> {
        if operation.quantity_delta == 0 && operation.action == "use" {
            if let Some(item) = inventory
                .iter_mut()
                .find(|item| item.item_id == operation.item_id)
            {
                for disclosed in &operation.disclosed_to_add {
                    if !item.disclosed_to.contains(disclosed) {
                        item.disclosed_to.push(disclosed.clone());
                    }
                }
            }
            return inventory;
        }

        if let Some(item) = inventory
            .iter_mut()
            .find(|item| item.item_id == operation.item_id)
        {
            item.quantity = (item.quantity + operation.quantity_delta).max(0);
            if operation.action == "add" {
                item.owner_type = operation.owner_type.clone();
                item.owner_id = operation.owner_id.clone();
                item.visibility = operation.visibility.clone();
            }
            for disclosed in &operation.disclosed_to_add {
                if !item.disclosed_to.contains(disclosed) {
                    item.disclosed_to.push(disclosed.clone());
                }
            }
            inventory.retain(|item| item.quantity > 0);
            return inventory;
        }

        if operation.action == "add" && operation.quantity_delta > 0 {
            inventory.push(InventoryItem {
                item_id: operation.item_id.clone(),
                name: operation.item_name.clone(),
                category: operation.category.clone(),
                quantity: operation.quantity_delta,
                description: operation.description.clone(),
                tags: operation.tags.clone(),
                owner_type: operation.owner_type.clone(),
                owner_id: operation.owner_id.clone(),
                visibility: operation.visibility.clone(),
                disclosed_to: operation.disclosed_to_add.clone(),
            });
        }
        inventory
    }
}
