from dataclasses import dataclass, field
import re
import uuid

from backend.app.domain.models.inventory import InventoryItem
from backend.app.domain.models.session import SessionSnapshot


@dataclass(frozen=True)
class InventoryOperation:
    action: str
    item_id: str
    item_name: str
    category: str
    quantity_delta: int
    reason: str
    description: str = ""
    tags: list[str] = field(default_factory=list)
    owner_type: str = "player"
    owner_id: str = "player"
    visibility: str = "private"
    disclosed_to_add: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class InventoryRuntimeResult:
    inventory_items: list[InventoryItem]
    operations: list[InventoryOperation] = field(default_factory=list)
    system_messages: list[str] = field(default_factory=list)
    debug_lines: list[str] = field(default_factory=list)


class InventoryRuntimeService:
    PICK_KEYWORDS = ["观察", "搜索", "搜查", "拾取", "拿起", "获得", "捡起"]
    USE_KEYWORDS = ["使用", "出示", "打开", "启动", "服用", "查看"]
    TRANSFER_KEYWORDS = ["交给", "交出", "递给", "交换", "放下", "丢下", "丢弃", "上交"]
    DISCLOSE_KEYWORDS = ["告诉", "展示", "给", "让", "说明", "亮出"]
    NON_CONSUMING_CATEGORIES = {"key-item", "device", "tool"}
    CONSUMING_CATEGORIES = {"clue", "consumable", "material"}
    QUANTITY_UNITS = "个张份件把瓶枚次组块包套"
    CHINESE_NUMBERS = {
        "一": 1,
        "二": 2,
        "两": 2,
        "三": 3,
        "四": 4,
        "五": 5,
        "六": 6,
        "七": 7,
        "八": 8,
        "九": 9,
    }

    def evaluate_turn(
        self,
        session: SessionSnapshot,
        player_input: str,
        location_override: str | None = None,
    ) -> InventoryRuntimeResult:
        current_location = location_override or session.location
        operations: list[InventoryOperation] = []
        working_inventory = list(session.inventory_items)
        player_visible_inventory = self._filter_player_visible_inventory(working_inventory)

        # 这里先产出结构化操作清单，再统一落到库存，后续可以直接替换成 LLM 的 schema 输出。
        pickup_operation = self._resolve_pickup_operation(
            current_location=current_location,
            player_input=player_input,
        )
        if pickup_operation is not None:
            operations.append(pickup_operation)
            working_inventory = self._apply_operation(working_inventory, pickup_operation)

        transfer_operation = self._resolve_transfer_operation(
            inventory=player_visible_inventory,
            player_input=player_input,
        )
        if transfer_operation is not None:
            operations.append(transfer_operation)
            working_inventory = self._apply_operation(working_inventory, transfer_operation)
        else:
            use_operation = self._resolve_use_operation(
                inventory=player_visible_inventory,
                player_input=player_input,
                visible_characters=session.visible_characters,
            )
            if use_operation is not None:
                operations.append(use_operation)
                working_inventory = self._apply_operation(working_inventory, use_operation)

        debug_lines = self._build_debug_lines(
            current_location=current_location,
            operations=operations,
        )
        system_messages = self._build_system_messages(operations)

        return InventoryRuntimeResult(
            inventory_items=working_inventory,
            operations=operations,
            system_messages=system_messages,
            debug_lines=debug_lines,
        )

    def _filter_player_visible_inventory(self, inventory: list[InventoryItem]) -> list[InventoryItem]:
        return [
            item
            for item in inventory
            if (item.owner_type == "player" and item.owner_id == "player")
            or item.visibility == "public"
        ]

    def _resolve_pickup_operation(
        self,
        current_location: str,
        player_input: str,
    ) -> InventoryOperation | None:
        if not any(keyword in player_input for keyword in self.PICK_KEYWORDS):
            return None

        found_item = self._resolve_pickup_item(current_location=current_location)
        if found_item is None:
            return None

        quantity = self._resolve_requested_quantity(player_input=player_input, fallback=found_item.quantity)
        return InventoryOperation(
            action="add",
            item_id=found_item.item_id,
            item_name=found_item.name,
            category=found_item.category,
            quantity_delta=quantity,
            reason=f"scene_pickup:{current_location}",
            description=found_item.description,
            tags=found_item.tags,
            owner_type=found_item.owner_type,
            owner_id=found_item.owner_id,
            visibility=found_item.visibility,
        )

    def _resolve_transfer_operation(
        self,
        inventory: list[InventoryItem],
        player_input: str,
    ) -> InventoryOperation | None:
        if not any(keyword in player_input for keyword in self.TRANSFER_KEYWORDS):
            return None

        candidate = self._resolve_target_item(inventory=inventory, player_input=player_input)
        if candidate is None:
            return None

        quantity = self._resolve_requested_quantity(
            player_input=player_input,
            max_quantity=candidate.quantity,
            fallback=1,
        )
        return InventoryOperation(
            action="transfer",
            item_id=candidate.item_id,
            item_name=candidate.name,
            category=candidate.category,
            quantity_delta=-quantity,
            reason="player_transfer",
            description=candidate.description,
            tags=candidate.tags,
            owner_type=candidate.owner_type,
            owner_id=candidate.owner_id,
            visibility=candidate.visibility,
        )

    def _resolve_use_operation(
        self,
        inventory: list[InventoryItem],
        player_input: str,
        visible_characters: list[str],
    ) -> InventoryOperation | None:
        if not any(keyword in player_input for keyword in [*self.USE_KEYWORDS, *self.DISCLOSE_KEYWORDS]):
            return None

        candidate = self._resolve_target_item(inventory=inventory, player_input=player_input)
        if candidate is None:
            return None

        quantity = self._resolve_requested_quantity(
            player_input=player_input,
            max_quantity=candidate.quantity,
            fallback=1,
        )
        quantity_delta = -quantity if self._should_consume_on_use(candidate, player_input) else 0
        reason = "player_use_consumed" if quantity_delta < 0 else "player_use_recorded"
        disclosed_targets = self._resolve_disclosed_targets(
            visible_characters=visible_characters,
            player_input=player_input,
            owner_type=candidate.owner_type,
            owner_id=candidate.owner_id,
        )

        return InventoryOperation(
            action="use",
            item_id=candidate.item_id,
            item_name=candidate.name,
            category=candidate.category,
            quantity_delta=quantity_delta,
            reason=reason,
            description=candidate.description,
            tags=candidate.tags,
            owner_type=candidate.owner_type,
            owner_id=candidate.owner_id,
            visibility=candidate.visibility,
            disclosed_to_add=disclosed_targets,
        )

    def _resolve_pickup_item(self, current_location: str) -> InventoryItem | None:
        if "青藤商场" in current_location:
            return InventoryItem(
                item_id=f"item-{uuid.uuid4().hex[:8]}",
                name="青藤通行卡",
                category="key-item",
                quantity=1,
                description="用于打开商场内部权限门的临时通行卡。",
                tags=["access", "scene:mall"],
                owner_type="player",
                owner_id="player",
                visibility="private",
            )

        if "避难站" in current_location or "高架桥" in current_location:
            return InventoryItem(
                item_id=f"item-{uuid.uuid4().hex[:8]}",
                name="潮湿图纸",
                category="clue",
                quantity=1,
                description="记录着局部电力切断线路的旧图纸。",
                tags=["clue", "electricity"],
                owner_type="player",
                owner_id="player",
                visibility="private",
            )

        if "藏经阁" in current_location:
            return InventoryItem(
                item_id=f"item-{uuid.uuid4().hex[:8]}",
                name="残页拓片",
                category="clue",
                quantity=1,
                description="从藏经阁里找到的残页拓片。",
                tags=["clue", "scripture"],
                owner_type="player",
                owner_id="player",
                visibility="private",
            )

        return None

    def _resolve_target_item(
        self,
        inventory: list[InventoryItem],
        player_input: str,
    ) -> InventoryItem | None:
        scored_candidates = [
            (self._score_item_match(item=item, player_input=player_input), item) for item in inventory
        ]
        scored_candidates = [item for item in scored_candidates if item[0] > 0]

        if not scored_candidates:
            return None

        scored_candidates.sort(key=lambda entry: entry[0], reverse=True)
        return scored_candidates[0][1]

    def _score_item_match(self, item: InventoryItem, player_input: str) -> int:
        if item.name in player_input:
            return 1000 + len(item.name)

        score = 0
        for tag in item.tags:
            if tag and tag in player_input:
                score = max(score, 400 + len(tag))

        name = item.name.strip()
        for size in range(min(len(name), 4), 1, -1):
            for start in range(0, len(name) - size + 1):
                fragment = name[start : start + size]
                if fragment and fragment in player_input:
                    score = max(score, 100 + size)

        return score

    def _resolve_requested_quantity(
        self,
        player_input: str,
        max_quantity: int | None = None,
        fallback: int = 1,
    ) -> int:
        if max_quantity is not None and ("全部" in player_input or "所有" in player_input):
            return max(1, max_quantity)

        match = re.search(rf"([0-9]+)\s*([{self.QUANTITY_UNITS}])", player_input)
        if match is None:
            match = re.search(r"[xX×]\s*([0-9]+)", player_input)
        if match is not None:
            value = int(match.group(1))
            return self._clamp_quantity(value=value, max_quantity=max_quantity, fallback=fallback)

        chinese_match = re.search(
            rf"(十[一二两三四五六七八九]?|[一二两三四五六七八九]十[一二两三四五六七八九]?|[一二两三四五六七八九])\s*([{self.QUANTITY_UNITS}])",
            player_input,
        )
        if chinese_match is not None:
            value = self._parse_chinese_number(chinese_match.group(1))
            return self._clamp_quantity(value=value, max_quantity=max_quantity, fallback=fallback)

        return self._clamp_quantity(value=fallback, max_quantity=max_quantity, fallback=fallback)

    def _parse_chinese_number(self, raw_value: str) -> int:
        if raw_value == "十":
            return 10
        if "十" not in raw_value:
            return self.CHINESE_NUMBERS.get(raw_value, 1)
        if raw_value.startswith("十"):
            return 10 + self.CHINESE_NUMBERS.get(raw_value[-1], 0)
        if raw_value.endswith("十"):
            return self.CHINESE_NUMBERS.get(raw_value[0], 1) * 10
        return self.CHINESE_NUMBERS.get(raw_value[0], 1) * 10 + self.CHINESE_NUMBERS.get(raw_value[-1], 0)

    def _clamp_quantity(self, value: int, max_quantity: int | None, fallback: int) -> int:
        if value <= 0:
            return fallback
        if max_quantity is not None:
            return max(1, min(value, max_quantity))
        return value

    def _should_consume_on_use(self, item: InventoryItem, player_input: str) -> bool:
        if "出示" in player_input:
            return False
        if item.category in self.NON_CONSUMING_CATEGORIES:
            return False
        if item.category in self.CONSUMING_CATEGORIES:
            return True
        return any(tag in {"consumable", "single-use"} for tag in item.tags)

    def _resolve_disclosed_targets(
        self,
        visible_characters: list[str],
        player_input: str,
        owner_type: str,
        owner_id: str,
    ) -> list[str]:
        if owner_type != "player" or owner_id != "player":
            return []
        if not any(keyword in player_input for keyword in self.DISCLOSE_KEYWORDS):
            return []

        return [name for name in visible_characters if name in player_input]

    def _apply_operation(
        self,
        inventory: list[InventoryItem],
        operation: InventoryOperation,
    ) -> list[InventoryItem]:
        if operation.action == "add":
            return self._add_or_increment(
                inventory=inventory,
                new_item=InventoryItem(
                    item_id=operation.item_id,
                    name=operation.item_name,
                    category=operation.category,
                    quantity=max(1, operation.quantity_delta),
                    description=operation.description,
                    tags=operation.tags,
                    owner_type=operation.owner_type,
                    owner_id=operation.owner_id,
                    visibility=operation.visibility,
                    disclosed_to=list(operation.disclosed_to_add),
                ),
            )

        updated: list[InventoryItem] = []
        for item in inventory:
            if item.item_id != operation.item_id:
                updated.append(item)
                continue

            next_disclosed_to = list(dict.fromkeys([*item.disclosed_to, *operation.disclosed_to_add]))

            if operation.quantity_delta >= 0:
                updated.append(
                    InventoryItem(
                        item_id=item.item_id,
                        name=item.name,
                        category=item.category,
                        quantity=item.quantity,
                        description=item.description,
                        tags=item.tags,
                        owner_type=item.owner_type,
                        owner_id=item.owner_id,
                        visibility=item.visibility,
                        disclosed_to=next_disclosed_to,
                    )
                )
                continue

            next_quantity = item.quantity + operation.quantity_delta
            if next_quantity > 0:
                updated.append(
                    InventoryItem(
                        item_id=item.item_id,
                        name=item.name,
                        category=item.category,
                        quantity=next_quantity,
                        description=item.description,
                        tags=item.tags,
                        owner_type=item.owner_type,
                        owner_id=item.owner_id,
                        visibility=item.visibility,
                        disclosed_to=next_disclosed_to,
                    )
                )

        return updated

    def _add_or_increment(self, inventory: list[InventoryItem], new_item: InventoryItem) -> list[InventoryItem]:
        updated: list[InventoryItem] = []
        matched = False

        for item in inventory:
            if (
                item.name == new_item.name
                and item.category == new_item.category
                and item.owner_type == new_item.owner_type
                and item.owner_id == new_item.owner_id
            ):
                matched = True
                updated.append(
                    InventoryItem(
                        item_id=item.item_id,
                        name=item.name,
                        category=item.category,
                        quantity=item.quantity + new_item.quantity,
                        description=item.description,
                        tags=item.tags,
                        owner_type=item.owner_type,
                        owner_id=item.owner_id,
                        visibility=item.visibility,
                        disclosed_to=list(dict.fromkeys([*item.disclosed_to, *new_item.disclosed_to])),
                    )
                )
            else:
                updated.append(item)

        if not matched:
            updated.append(new_item)

        return updated

    def _build_system_messages(self, operations: list[InventoryOperation]) -> list[str]:
        messages: list[str] = []
        for operation in operations:
            quantity = abs(operation.quantity_delta)
            quantity_suffix = f" x{quantity}" if quantity > 1 else ""
            disclosed_suffix = (
                f"，已向 {' / '.join(operation.disclosed_to_add)} 公开"
                if operation.disclosed_to_add
                else ""
            )

            if operation.action == "add":
                messages.append(f"背包系统：获得 {operation.item_name}{quantity_suffix}")
            elif operation.action == "transfer":
                messages.append(f"背包系统：交出 {operation.item_name}{quantity_suffix}")
            elif operation.action == "use" and operation.quantity_delta < 0:
                messages.append(f"背包系统：使用 {operation.item_name}{quantity_suffix}，已扣除数量{disclosed_suffix}")
            elif operation.action == "use":
                messages.append(f"背包系统：使用 {operation.item_name}{disclosed_suffix}")

        return messages

    def _build_debug_lines(
        self,
        current_location: str,
        operations: list[InventoryOperation],
    ) -> list[str]:
        if not operations:
            return [f"InventoryRuntime no_op location={current_location}"]

        return [
            "InventoryRuntime "
            + f"op={operation.action} item={operation.item_name} delta={operation.quantity_delta:+d} reason={operation.reason}"
            for operation in operations
        ]
