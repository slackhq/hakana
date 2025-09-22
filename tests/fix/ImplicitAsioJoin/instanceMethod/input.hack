final class TestClass {
    public async function async_method(): Awaitable<int> {
        await \HH\Asio\usleep(100000);
        return 42;
    }

    // This sync method just wraps the async version
    public function sync_method(): int {
        return Asio\join($this->async_method());
    }

    public static function factory(): TestClass {
        return new self();
    }
}

function caller(): int {
    $obj = new TestClass();
    // This should be fixed to Asio\join($obj->async_method()) instead.
    return $obj->sync_method();
}

function caller_expr(): int {
    // This should be fixed to Asio\join((new TestClass())->async_method()) instead.
    return (new TestClass())->sync_method();
}

function factory_expr(): int {
    // This should be fixed to Asio\join(TestClass::factory()->async_method()) instead.
    return TestClass::factory()->sync_method();
}

async function async_caller(): int {
    $obj = new TestClass();
    // This should be fixed to await $obj->async_method() instead.
    return $obj->sync_method();
}

async function async_caller_expr(): int {
    // This should be fixed to await (new TestClass())->async_method() instead.
    return (new TestClass())->sync_method();
}

async function async_factory_expr(): int {
    // This should be fixed to await TestClass::factory()->async_method() instead.
    return TestClass::factory()->sync_method();
}