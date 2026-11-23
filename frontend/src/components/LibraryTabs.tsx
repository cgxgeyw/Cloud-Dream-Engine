import { useNavigate } from "react-router-dom";
import "./LibraryTabs.css";

type LibraryTab = "knowledge" | "stickers";

export function LibraryTabs({ active }: { active: LibraryTab }) {
  const navigate = useNavigate();
  return (
    <nav className="library-tabs" aria-label="图库和知识库">
      <button
        type="button"
        className={`library-tab${active === "knowledge" ? " library-tab--active" : ""}`}
        aria-current={active === "knowledge" ? "page" : undefined}
        onClick={() => navigate("/knowledge")}
      >
        知识库
      </button>
      <button
        type="button"
        className={`library-tab${active === "stickers" ? " library-tab--active" : ""}`}
        aria-current={active === "stickers" ? "page" : undefined}
        onClick={() => navigate("/stickers")}
      >
        图库
      </button>
    </nav>
  );
}
