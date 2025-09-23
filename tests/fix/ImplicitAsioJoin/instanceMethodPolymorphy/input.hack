abstract class TestClass {
    public function __construct(private int $x);

    public async function async_method(): Awaitable<int> {
        await \HH\Asio\usleep(100000);
        return $this->x;
    }

    // This sync method just wraps the async version
    public function sync_method(): int {
        return Asio\join($this->async_method());
    }
}

final class A extends TestClass {}
final class B extends TestClass {}

function polymorphic_fn(bool $something): Awaitable<void> {
    if ($something) {
        $obj = new A();
    } else {
        $obj = new B();
    }

    // This should be fixed to Asio\join($obj->async_method())
    $obj->sync_method();
}

async function async_polymorphic_fn(bool $something): Awaitable<void> {
    if ($something) {
        $obj = new A();
    } else {
        $obj = new B();
    }

    // This should be fixed to await $obj->async_method()
    $obj->sync_method();
}
