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

function foreach_fn(vec<int> $something): void {
    $obj = new TestClass();
    foreach ($something as $_) {
        $obj = new TestClass();
        // This should be fixed to Asio\join($obj->async_method())
        $foo = $obj->sync_method();
    }
}

async function async_foreach_fn(vec<int> $something): Awaitable<void> {
    $obj = new TestClass();
    foreach ($something as $_) {
        $obj = new TestClass();
         // This should be fixed to await $obj->async_method()
        $foo = $obj->sync_method();
    }
}
