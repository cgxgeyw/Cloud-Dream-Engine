from backend.app.core.container import AppContainer, get_container


def get_app_container() -> AppContainer:
    return get_container()
