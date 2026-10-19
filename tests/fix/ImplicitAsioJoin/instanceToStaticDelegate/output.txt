final class TokenProvider {
    public static async function fetch(bool $refresh): Awaitable<string> {
        return $refresh ? 'new' : 'cached';
    }
}

async function fetch_token(bool $refresh): Awaitable<string> {
    return $refresh ? 'new' : 'cached';
}

final class Client {
    public function fetch(bool $refresh): string {
        return \HH\Asio\join(TokenProvider::fetch($refresh));
    }

    public function fetchFunction(bool $refresh): string {
        return \HH\Asio\join(fetch_token($refresh));
    }
}

async function run(Client $client): Awaitable<void> {
    echo $client->fetch(false);
    echo (new Client())->fetch(true);
    echo $client->fetchFunction(false);
}
