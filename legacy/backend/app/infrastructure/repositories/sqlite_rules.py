import uuid

from backend.app.domain.models.rule import RuleDefinition
from backend.app.infrastructure.sqlite_store import SqliteStore, row_to_rule


class SqliteRuleRepository:
    def __init__(self, store: SqliteStore) -> None:
        self._store = store

    def list_rules(self, scope: str | None = None) -> list[RuleDefinition]:
        with self._store.connect() as connection:
            if scope:
                rows = connection.execute(
                    "SELECT * FROM rules WHERE scope = ? ORDER BY priority DESC, name",
                    (scope,),
                ).fetchall()
            else:
                rows = connection.execute("SELECT * FROM rules ORDER BY priority DESC, name").fetchall()
        return [row_to_rule(row) for row in rows]

    def get_rule(self, rule_id: str) -> RuleDefinition | None:
        with self._store.connect() as connection:
            row = connection.execute("SELECT * FROM rules WHERE id = ?", (rule_id,)).fetchone()
        return row_to_rule(row) if row else None

    def create_rule(self, rule: RuleDefinition) -> RuleDefinition:
        created = RuleDefinition(
            id=rule.id if rule.id and rule.id != "new" else f"rule-{uuid.uuid4().hex[:8]}",
            scope=rule.scope,
            name=rule.name,
            enabled=rule.enabled,
            priority=rule.priority,
            description=rule.description,
            condition=rule.condition,
            effects=rule.effects,
        )
        with self._store.connect() as connection:
            self._store.insert_rule(connection, created)
        return created

    def update_rule(self, rule_id: str, rule: RuleDefinition) -> RuleDefinition | None:
        if self.get_rule(rule_id) is None:
            return None
        updated = RuleDefinition(
            id=rule_id,
            scope=rule.scope,
            name=rule.name,
            enabled=rule.enabled,
            priority=rule.priority,
            description=rule.description,
            condition=rule.condition,
            effects=rule.effects,
        )
        with self._store.connect() as connection:
            self._store.upsert_rule(connection, updated)
        return updated
