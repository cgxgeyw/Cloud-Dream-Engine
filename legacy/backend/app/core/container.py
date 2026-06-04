from dataclasses import dataclass
from functools import lru_cache

from backend.app.application.services.agent_conversation_runtime_service import AgentConversationRuntimeService
from backend.app.application.services.agent_runtime_manager_service import AgentRuntimeManagerService
from backend.app.application.services.attribute_runtime_service import AttributeRuntimeService
from backend.app.application.services.attribute_service import AttributeCommandService, AttributeQueryService
from backend.app.application.services.asset_resolver_service import AssetResolverService
from backend.app.application.services.catalog_service import CatalogCommandService, CatalogQueryService
from backend.app.application.services.character_runtime_service import CharacterRuntimeService
from backend.app.application.services.dialogue_pipeline_service import DialoguePipelineService
from backend.app.application.services.image_generation_service import ImageGenerationService
from backend.app.application.services.inventory_runtime_service import InventoryRuntimeService
from backend.app.application.services.memory_service import MemoryCommandService, MemoryQueryService
from backend.app.application.services.narration_service import NarrationService
from backend.app.application.services.runtime_visibility_service import RuntimeVisibilityService
from backend.app.application.services.rule_engine_service import RuleEngineService
from backend.app.application.services.scene_runtime_manager_service import SceneRuntimeManagerService
from backend.app.application.services.session_orchestrator_service import SessionOrchestratorService
from backend.app.application.services.session_service import SessionCommandService, SessionQueryService
from backend.app.application.services.state_engine_service import StateEngineService
from backend.app.application.services.text_generation_service import TextGenerationService
from backend.app.application.services.trigger_engine_service import TriggerEngineService
from backend.app.application.services.world_director_service import WorldDirectorService
from backend.app.infrastructure.repositories.sqlite_attributes import SqliteAttributeRepository
from backend.app.core.config import Settings
from backend.app.infrastructure.repositories.sqlite_agent_runtime import SqliteAgentRuntimeRepository
from backend.app.infrastructure.repositories.sqlite_catalog import SqliteCatalogRepository
from backend.app.infrastructure.repositories.sqlite_memory import SqliteMemoryRepository
from backend.app.infrastructure.repositories.sqlite_rules import SqliteRuleRepository
from backend.app.infrastructure.repositories.sqlite_sessions import SqliteSessionRepository
from backend.app.infrastructure.sqlite_store import SqliteStore


@dataclass(frozen=True)
class AppContainer:
    attribute_queries: AttributeQueryService
    attribute_commands: AttributeCommandService
    agent_runtime_manager: AgentRuntimeManagerService
    attribute_runtime: AttributeRuntimeService
    asset_resolver: AssetResolverService
    memory_queries: MemoryQueryService
    memory_commands: MemoryCommandService
    runtime_visibility: RuntimeVisibilityService
    world_director: WorldDirectorService
    dialogue_pipeline: DialoguePipelineService
    catalog_queries: CatalogQueryService
    catalog_commands: CatalogCommandService
    session_queries: SessionQueryService
    session_commands: SessionCommandService
    session_orchestrator: SessionOrchestratorService
    session_runtime: SqliteSessionRepository


@lru_cache(maxsize=1)
def get_container() -> AppContainer:
    settings = Settings()
    store = SqliteStore(settings.database_path)
    attribute_repository = SqliteAttributeRepository(store=store)
    agent_runtime_repository = SqliteAgentRuntimeRepository(store=store)
    memory_repository = SqliteMemoryRepository(store=store)
    rule_repository = SqliteRuleRepository(store=store)
    catalog_repository = SqliteCatalogRepository(store=store)
    session_repository = SqliteSessionRepository(
        store=store,
        catalog_repository=catalog_repository,
        attribute_repository=attribute_repository,
    )
    catalog_queries = CatalogQueryService(catalog_repository=catalog_repository)
    catalog_commands = CatalogCommandService(catalog_repository=catalog_repository)
    attribute_runtime = AttributeRuntimeService(
        attribute_repository=attribute_repository,
        catalog_repository=catalog_repository,
        session_repository=session_repository,
    )
    text_generation = TextGenerationService(catalog_queries=catalog_queries)
    agent_runtime_manager = AgentRuntimeManagerService(runtime_repository=agent_runtime_repository)
    world_director = WorldDirectorService(
        text_generation=text_generation,
        attribute_runtime=attribute_runtime,
    )
    trigger_engine = TriggerEngineService()
    rule_engine = RuleEngineService(rule_repository=rule_repository)
    scene_runtime_manager = SceneRuntimeManagerService()
    state_engine = StateEngineService()
    narration_service = NarrationService()
    dialogue_pipeline = DialoguePipelineService(text_generation=text_generation)
    agent_conversation_runtime = AgentConversationRuntimeService(
        runtime_manager=agent_runtime_manager,
        text_generation=text_generation,
        world_director=world_director,
        dialogue_pipeline=dialogue_pipeline,
    )
    character_runtime = CharacterRuntimeService(
        dialogue_pipeline=dialogue_pipeline,
        agent_conversation_runtime=agent_conversation_runtime,
    )
    image_generation = ImageGenerationService(catalog_queries=catalog_queries, settings=settings)
    asset_resolver = AssetResolverService(catalog_queries=catalog_queries, image_generation=image_generation)
    inventory_runtime = InventoryRuntimeService()
    runtime_visibility = RuntimeVisibilityService()
    session_orchestrator = SessionOrchestratorService(
        session_repository=session_repository,
        catalog_queries=catalog_queries,
        catalog_commands=catalog_commands,
        attribute_queries=AttributeQueryService(attribute_repository=attribute_repository),
        attribute_commands=AttributeCommandService(attribute_repository=attribute_repository),
        agent_runtime_manager=agent_runtime_manager,
        agent_conversation_runtime=agent_conversation_runtime,
        attribute_runtime=attribute_runtime,
        asset_resolver=asset_resolver,
        inventory_runtime=inventory_runtime,
        memory_queries=MemoryQueryService(memory_repository=memory_repository),
        memory_commands=MemoryCommandService(memory_repository=memory_repository),
        runtime_visibility=runtime_visibility,
        world_director=world_director,
        scene_runtime_manager=scene_runtime_manager,
        trigger_engine=trigger_engine,
        rule_engine=rule_engine,
        state_engine=state_engine,
        narration_service=narration_service,
        character_runtime=character_runtime,
    )

    session_queries = SessionQueryService(session_repository=session_repository)
    session_commands = SessionCommandService(session_repository=session_repository)

    return AppContainer(
        attribute_queries=AttributeQueryService(attribute_repository=attribute_repository),
        attribute_commands=AttributeCommandService(attribute_repository=attribute_repository),
        agent_runtime_manager=agent_runtime_manager,
        attribute_runtime=attribute_runtime,
        asset_resolver=asset_resolver,
        memory_queries=MemoryQueryService(memory_repository=memory_repository),
        memory_commands=MemoryCommandService(memory_repository=memory_repository),
        runtime_visibility=runtime_visibility,
        world_director=world_director,
        dialogue_pipeline=dialogue_pipeline,
        catalog_queries=catalog_queries,
        catalog_commands=catalog_commands,
        session_queries=session_queries,
        session_commands=session_commands,
        session_orchestrator=session_orchestrator,
        session_runtime=session_repository,
    )
