async function do_async_work(): Awaitable<int> {
    return 42;
}

abstract class Base {
    abstract public function get_data(): int;
}

final class Child extends Base {
    <<__Override>>
    public function get_data(): int {
        $result = \HH\Asio\join(do_async_work());
        return $result + 1;
    }
}

async function caller(Base $b): Awaitable<int> {
    $x = await do_async_work();
    return $b->get_data() + $x;
}
