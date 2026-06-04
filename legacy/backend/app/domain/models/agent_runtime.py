from dataclasses import dataclass, field
from enum import Enum


class _StrEnumBase(str, Enum):
    """Compatibility StrEnum for Python 3.10."""
    def __str__(self) -> str:
        return self.value


class AgentSessionStatus(_StrEnumBase):
    PENDING_INIT = "pending_init"
    ACTIVE = "active"
    INACTIVE = "inactive"
    EVICTED = "evicted"
    FAILED = "failed"
    CLOSED = "closed"


class AgentType(_StrEnumBase):
    DIRECTOR = "director"
    CHARACTER = "character"


class ConnectionState(_StrEnumBase):
    DISCONNECTED = "disconnected"
    CONNECTED = "connected"
    RECOVERING = "recovering"
    FAILED = "failed"


class ScenePresenceState(_StrEnumBase):
    PRESENT = "present"
    OFFSTAGE = "offstage"
    UNKNOWN = "unknown"


class CheckpointType(_StrEnumBase):
    INITIALIZATION = "initialization"
    TURN_STATE = "turn_state"
    RECOVERY = "recovery"


class TurnStepStatus(_StrEnumBase):
    CREATED = "created"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    SKIPPED = "skipped"


# Backward-compatible string sets for validation
AGENT_SESSION_STATUSES = set(AgentSessionStatus)
AGENT_TYPES = set(AgentType)
CONNECTION_STATES = set(ConnectionState)
SCENE_PRESENCE_STATES = set(ScenePresenceState)
CHECKPOINT_TYPES = set(CheckpointType)
TURN_STEP_STATUSES = set(TurnStepStatus)


@dataclass(frozen=True)
class AgentSession:
    id: str
    session_id: str
    agent_type: str
    status: str
    connection_state: str
    scene_presence_state: str
    character_id: str | None = None
    character_name: str | None = None
    checkpoint_id: str | None = None
    last_active_turn: int = 0
    last_ack_message_index: int = 0
    prompt_version: str = "v1"
    runtime_key: str | None = None
    initialized_at: str | None = None
    created_at: str = ""
    updated_at: str = ""


@dataclass(frozen=True)
class AgentCheckpoint:
    id: str
    agent_session_id: str
    turn_index: int
    checkpoint_type: str
    payload: dict[str, object] = field(default_factory=dict)
    created_at: str = ""


@dataclass(frozen=True)
class TurnJournalEntry:
    id: str
    session_id: str
    turn_index: int
    step: str
    status: str
    payload: dict[str, object] = field(default_factory=dict)
    created_at: str = ""
