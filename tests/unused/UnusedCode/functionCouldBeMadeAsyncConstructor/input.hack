async function do_async_work(): Awaitable<int> {
    return 42;
}

final class Foo {
    private int $value;

    public function __construct() {
        $this->value = \HH\Asio\join(do_async_work());
    }

    public function get_value(): int {
        return $this->value;
    }
}

async function caller(): Awaitable<int> {
    $foo = new Foo();
    $x = await do_async_work();
    return $foo->get_value() + $x;
}
