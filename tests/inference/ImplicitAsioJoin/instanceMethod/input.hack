final class TestClass {
    public async function async_method(): Awaitable<int> {
        await \HH\Asio\usleep(100000);
        return 42;
    }

    public function sync_method(): int {
        return Asio\join($this->async_method()); // This should be detected by scanner
    } 
}

function caller(): int {
    $obj = new TestClass();
    return $obj->sync_method(); // This should trigger ImplicitAsioJoin
}