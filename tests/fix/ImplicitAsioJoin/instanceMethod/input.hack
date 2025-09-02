final class TestClass {
    public async function async_method(): Awaitable<int> {
        await \HH\Asio\usleep(100000);
        return 42;
    }

    // This sync method just wraps the async version
    public function sync_method(): int {
        return Asio\join($this->async_method());
    } 
}

function caller(): int {
    $obj = new TestClass();
    // This should be fixed to Asio\join($obj->async_method()) instead.
    return $obj->sync_method();
}

async function async caller(): int {
    $obj = new TestClass();
    // This should be fixed to await $obj->async_method() instead.
    return $obj->sync_method();
}