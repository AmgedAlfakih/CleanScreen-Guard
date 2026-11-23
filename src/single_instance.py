from PySide6.QtCore import QObject, Signal
from PySide6.QtNetwork import QLocalServer, QLocalSocket


class SingleInstanceGuard(QObject):
    """
    Ensures only one copy of CleanScreen Guard runs at a time.
    Double-clicking the shortcut while already running wakes the app and opens Settings.
    """

    activate_requested = Signal()

    def __init__(self, key: str = "CleanScreenGuard_SingleInstance_v1", parent=None) -> None:
        super().__init__(parent)
        self._key = key
        self._server: QLocalServer | None = None

    def try_acquire(self) -> bool:
        socket = QLocalSocket()
        socket.connectToServer(self._key)
        if socket.waitForConnected(250):
            socket.write(b"SHOW_SETTINGS")
            socket.flush()
            socket.waitForBytesWritten(250)
            socket.disconnectFromServer()
            return False

        QLocalServer.removeServer(self._key)
        self._server = QLocalServer(self)
        self._server.newConnection.connect(self._on_new_connection)
        self._server.listen(self._key)
        return True

    def _on_new_connection(self) -> None:
        if self._server is None:
            return
        client = self._server.nextPendingConnection()
        if client is None:
            return
        client.waitForReadyRead(200)
        client.readAll()
        client.disconnectFromServer()
        client.deleteLater()
        self.activate_requested.emit()

    def close(self) -> None:
        if self._server is not None:
            self._server.close()
            self._server = None
