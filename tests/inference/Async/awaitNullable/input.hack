final class A {
  public async function bar(): Awaitable<string> {
    await \HH\Asio\usleep(100000);
    return "a";
  }
}

final class Boo {
    public static async function getA(): Awaitable<?A> {
        await \HH\Asio\usleep(100000);
        return new A();
    }
}


function foo(): void {
    $a = await Boo::getA();
    $b = await ($a is nonnull ? $a->bar() : null);
    if ($b is nonnull) {}
}
