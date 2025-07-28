final class Box<T> {
    public function __construct(private T $t) {}

    public function get()[]: T {
        return $this->t;
    }
}

async function foo(): Awaitable<Box<string>> {
    await \HH\Asio\usleep(100000);
    return new Box("hello");
}

async function bar(): Awaitable<void> {
    $result = await foo();
    $val = $result->get();
    if (rand(0, 1)) {
        echo $val;
    }
}