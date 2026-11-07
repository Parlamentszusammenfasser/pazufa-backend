import datetime
import argparse
import dotenv


class Key:
    material: str
    backend: str

    def __init__(self, material, backend) -> None:
        self.material = material
        self.backend = backend

    def __str__(self) -> str:
        return self.material


def curl(url: str, method: str, headers: list[str], body: str | None = None):
    pass


def curl_backend(
    key: Key,
    method: str,
    url_parts: str,
    headers: list[str],
    body: str | None = None,
):
    end_headers = headers
    end_headers.append(f"x-api-key: {key.material}")
    url = f"{key.backend}{url_parts}"
    curl(url, method, end_headers, body)


def revoke(key: Key, keytag: str):
    curl_backend(key, "DELETE", "/api/v2/auth", [f"api-key-delete: {keytag}"])


def create(key: Key, scope: str, keytag: str, expires_at: datetime.datetime):
    timestamp = expires_at.isoformat("YYYY-MM-DD HH:MM:SS")
    body = f'{{"scope": "{scope}", "expires_at": "{timestamp}", "keytag": "{keytag}"}}'
    curl_backend(key, "POST", "/api/v2/auth", [], body)


def list_keys(key: Key):
    curl_backend(key, "GET", "/api/v2/auth/keys", [])


def rotate(key: Key):
    curl_backend(key, "PUT", "/api/v2/auth/rotate", [])


if __name__ == "__main__":
    SCOPES = ["collector", "admin", "keyadder"]

    dotenv.load_dotenv()
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    delete_parser = subparsers.add_parser("delete")
    delete_parser.add_argument("keytag")

    rotate_parser = subparsers.add_parser("rotate")

    create_parser = subparsers.add_parser("create")
    create_parser.add_argument("scope", choices=SCOPES)
    create_parser.add_argument("--tag", dest="keytag")
    create_parser.add_argument("--expires")
    args = parser.parse_args()

    print(args)
