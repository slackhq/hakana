class A {
  public async function bar(): Awaitable<string> {
    return "a";
  }
}

class Boo {
    public static async function getA(): Awaitable<?A> {
        return new A();
    }
}


function foo(): void {
    $a = await Boo::getA();
    $b = await ($a is nonnull ? $a->bar() : null);
    if ($b is nonnull) {}
}
