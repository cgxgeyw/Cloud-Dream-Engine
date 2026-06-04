from backend.app.domain.repositories.session import SessionRepository


class SessionQueryService:
    def __init__(self, session_repository: SessionRepository) -> None:
        self._session_repository = session_repository

    def list_saves(self):
        return self._session_repository.list_saves()

    def get_session(self, session_id: str):
        return self._session_repository.get_session(session_id)


class SessionCommandService:
    def __init__(self, session_repository: SessionRepository) -> None:
        self._session_repository = session_repository

    def branch_save(self, save_id: str, branch_label: str | None = None):
        return self._session_repository.branch_save(save_id, branch_label=branch_label)

    def delete_save(self, save_id: str) -> bool:
        return self._session_repository.delete_save(save_id)

    def delete_all_saves(self) -> int:
        return self._session_repository.delete_all_saves()
