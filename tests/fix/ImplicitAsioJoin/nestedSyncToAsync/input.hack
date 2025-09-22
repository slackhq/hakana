final class Inner {
    public function foo(): void {
        echo "foo\n";
    }
}

final class TestClass {
    public async function async_method(): Awaitable<Inner> {
        await \HH\Asio\usleep(100000);
        return new Inner();
    }

    // This sync method just wraps the async version
    public function sync_method(): Inner {
        return Asio\join($this->async_method());
    }

    public async function async_vec(): Awaitable<vec<int>> {
        return vec[0];
    }

    // This sync method just wraps the async version
    public function sync_vec(): vec<int> {
        return Asio\join($this->async_vec());
    }

    public static function factory(): TestClass {
        return new self();
    }
}

function caller(): int {
    $obj = new TestClass();
    // This should be fixed to Asio\join($obj->async_method())->foo() instead.
    return $obj->sync_method()->foo();
}

function factory_expr(): int {
    // This should be fixed to Asio\join(TestClass::factory()->async_method())->foo() instead.
    return TestClass::factory()->sync_method()->foo();
}

async function async_caller(): int {
    $obj = new TestClass();
    // This should be fixed to (await $obj->async_method())->foo() instead.
    return $obj->sync_method()->foo();
}

async function async_factory_expr(): int {
    // This should be fixed to (await TestClass::factory()->async_method())->foo() instead.
    return TestClass::factory()->sync_method()->foo();
}

async function await_assignments(): Awaitable<void> {
    // This should be fixed to (await TestClass::factory()->async_method())->foo() instead.
    $foo = TestClass::factory()->sync_method()->foo();
    // This should be fixed to await TestClass::factory()->async_method() instead.
    $bar = TestClass::factory()->sync_method();
}

async function await_array_access(): Awaitable<void> {
    // This should be fixed to (await TestClass::factory()->async_vec())[0] instead.
    $test = TestClass::factory()->sync_vec()[0];
}