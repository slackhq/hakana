final class Svc {
    public async function fetch_async(): Awaitable<int> {
        return 1;
    }

    public function fetch(): int {
        return Asio\join($this->fetch_async());
    }
}
