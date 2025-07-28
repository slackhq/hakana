final class Box<T> {
    public function __construct(private T $t) {}

    public function get(): T {
        return $this->t;
    }
}

async function foo(): Awaitable<Box<string>> {
    await \HH\Asio\usleep(100000);
    return new Box("hello");
}

function takesBox(inout Box<string> $_b): void {}

async function bar(): Awaitable<void> {
    $result = await foo();
    takesBox(inout $result);
    if (rand(0, 1)) {
        echo $result->get();
    }
}