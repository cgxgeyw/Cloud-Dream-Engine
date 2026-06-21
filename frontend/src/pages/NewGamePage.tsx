import { useEffect, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  createSession,
  fetchWorld,
  fetchWorldCharacters,
  fetchWorlds,
  type CharacterResponse,
  type WorldResponse,
} from "../data/apiAdapter";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { useT } from "../data/i18n/context";
import { ArrowLeft } from "lucide-react";

function countMapNodes(value: WorldResponse["map_nodes"]): number {
  let count = 0;
  function visit(node: unknown) {
    if (!node || typeof node !== "object" || Array.isArray(node)) {
      return;
    }
    count += 1;
    const children = (node as { children?: unknown }).children;
    if (Array.isArray(children)) {
      children.forEach(visit);
    }
  }

  const root = value.root ?? value.tree;
  if (root) {
    visit(root);
  } else if (Array.isArray(value.nodes)) {
    value.nodes.forEach(visit);
  }
  return count;
}

function useWorldList(preferredWorldId?: string | null) {
  const t = useT();
  const [worlds, setWorlds] = useState<WorldResponse[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadWorlds() {
      try {
        setLoading(true);
        setError(null);
        const worldData = await fetchWorlds();
        if (!cancelled) {
          const sorted = preferredWorldId
            ? [...worldData].sort((left, right) => {
                if (left.id === preferredWorldId) return -1;
                if (right.id === preferredWorldId) return 1;
                return 0;
              })
            : worldData;
          setWorlds(sorted);
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : t("newGame.loadWorldsFailed"));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void loadWorlds();
    return () => {
      cancelled = true;
    };
  }, [preferredWorldId]);

  return { worlds, loading, error };
}

export function NewGamePage() {
  const navigate = useNavigate();
  const t = useT();
  const [searchParams] = useSearchParams();
  const preferredWorldId = searchParams.get("worldId");
  const { worlds, loading, error } = useWorldList(preferredWorldId);

  return (
    <ScreenLayout
      title={t("newGame.title")}
      subtitle={t("newGame.subtitle")}
      compactHeader
      maxWidth={980}
      toolbar={
        <button type="button" onClick={() => navigate(-1)} className="action-btn">
          <ArrowLeft size={14} /> {t("newGame.home")}
        </button>
      }
    >
      {loading ? <div className="empty-text">{t("newGame.loadingWorlds")}</div> : null}
      {error ? <div className="error-text">{t("newGame.loadError").replace("{error}", error)}</div> : null}

      {!loading && !error && worlds.length === 0 ? (
        <div className="empty-text">{t("newGame.noWorlds")}</div>
      ) : null}

      {!loading && !error && worlds.length > 0 ? (
        <div className="newgame-screen">
          <SurfacePanel className="surface-panel--pad-lg newgame-list-panel">
            <div className="newgame-list-head">
              <div>
                <strong className="newgame-section-title">{t("newGame.selectWorld")}</strong>
              </div>
              <button type="button" className="action-btn" onClick={() => navigate("/worlds")}>
                {t("newGame.manageWorlds")}
              </button>
            </div>

            <div className="newgame-world-grid">
              {worlds.map((world) => (
                <button
                  key={world.id}
                  type="button"
                  className={`newgame-world-card${world.id === preferredWorldId ? " newgame-world-card--active" : ""}`}
                  onClick={() => navigate(`/new-game/setup/${world.id}`)}
                >
                  <div className="newgame-world-card-top">
                    <strong>{world.name}</strong>
                    <span>{world.genre || t("newGame.uncategorized")}</span>
                  </div>
                  <p>{world.summary || t("newGame.noSummary")}</p>
                  <div className="newgame-world-card-meta">
                    <span>{t("newGame.openingPrefix").replace("{scene}", world.opening_scene || t("newGame.notSet"))}</span>
                    <span>{t("newGame.mapCount")} {countMapNodes(world.map_nodes)}</span>
                  </div>
                </button>
              ))}
            </div>
          </SurfacePanel>
        </div>
      ) : null}
    </ScreenLayout>
  );
}

export function NewGameSetupPage() {
  const navigate = useNavigate();
  const t = useT();
  const { worldId = "" } = useParams();
  const [world, setWorld] = useState<WorldResponse | null>(null);
  const [characters, setCharacters] = useState<CharacterResponse[]>([]);
  const [selectedPlayerCharacterId, setSelectedPlayerCharacterId] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    let cancelled = false;

    async function loadSetupData() {
      try {
        setLoading(true);
        setError(null);
        const [worldData, characterData] = await Promise.all([
          fetchWorld(worldId),
          fetchWorldCharacters(worldId),
        ]);
        if (!cancelled) {
          setWorld(worldData);
          setCharacters(characterData);
          setSelectedPlayerCharacterId(worldData.player_character_id ?? characterData[0]?.id ?? "");
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : t("newGame.loadSetupFailed"));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    if (worldId) {
      void loadSetupData();
    } else {
      setLoading(false);
      setError(t("newGame.missingWorldId"));
    }

    return () => {
      cancelled = true;
    };
  }, [worldId]);

  async function handleCreateSession() {
    if (!world) {
      return;
    }
    if (!selectedPlayerCharacterId) {
      setError(t("newGame.selectPlayerFirst"));
      return;
    }

    try {
      setCreating(true);
      setError(null);
      const session = await createSession({
        world_id: world.id,
        player_character_id: selectedPlayerCharacterId,
      });
      navigate(`/game/${session.id}`);
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : t("newGame.startSessionFailed"));
    } finally {
      setCreating(false);
    }
  }

  return (
    <ScreenLayout
      title={t("newGame.setupTitle")}
      subtitle={t("newGame.setupSubtitle")}
      compactHeader
      maxWidth={860}
      toolbar={
        <button type="button" onClick={() => navigate(-1)} className="action-btn">
          <ArrowLeft size={14} /> {t("newGame.back")}
        </button>
      }
    >
      {loading ? <div className="empty-text">{t("newGame.loadingSetup")}</div> : null}
      {error ? <div className="error-text">{t("newGame.loadError").replace("{error}", error)}</div> : null}

      {!loading && !error && world ? (
        <div className="newgame-screen newgame-setup-screen">
          <div className="newgame-setup-panel">


            <div className="newgame-preview-hero">
              <div className="newgame-label">{t("newGame.aboutToStart")}</div>
              <h2 className="newgame-world-name">{world.name}</h2>
              <div className="newgame-world-meta">{world.genre || t("newGame.uncategorizedWorld")}</div>
              <p className="newgame-preview-summary">{world.summary || t("newGame.noSummary")}</p>
            </div>

            <div className="newgame-info-grid">
              <div className="newgame-info-row">
                <strong>{t("newGame.openingLocation")}</strong>
                <span>{world.opening_scene || t("newGame.notSet")}</span>
              </div>
              <div className="newgame-info-row">
                <strong>{t("newGame.timeSystem")}</strong>
                <span>{world.time_system || t("newGame.notSet")}</span>
              </div>
              <div className="newgame-info-row newgame-info-row--wide">
                <strong>{t("newGame.presentCharacters")}</strong>
                <span>{characters.length > 0 ? characters.map((item) => item.name).join(" / ") : t("newGame.noCharacters")}</span>
              </div>
            </div>

            <div className="newgame-player-card">
              <label className="editor-field">
                <span className="editor-field-label">{t("newGame.selectYourCharacter")}</span>
                <select
                  value={selectedPlayerCharacterId}
                  onChange={(event) => setSelectedPlayerCharacterId(event.target.value)}
                  className="editor-field-input editor-field-select"
                >
                  {characters.map((character) => (
                    <option key={character.id} value={character.id}>
                      {character.name}
                      {world.player_character_id === character.id ? t("newGame.defaultSuffix") : ""}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <div className="newgame-action-stack">
              <div className="newgame-secondary-actions">
                <button type="button" className="action-btn" onClick={() => navigate(`/worlds/${world.id}/edit`)}>
                  {t("newGame.editWorld")}
                </button>
                <button
                  type="button"
                  onClick={() => navigate(`/worlds/${world.id}/characters`)}
                  className="action-btn"
                >
                  {t("newGame.characterPool")}
                </button>
              </div>
              <button
                type="button"
                onClick={() => void handleCreateSession()}
                disabled={creating || !selectedPlayerCharacterId}
                className="action-btn action-btn--primary newgame-start-btn"
              >
                {creating ? t("newGame.starting") : t("newGame.enterWorld")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </ScreenLayout>
  );
}
