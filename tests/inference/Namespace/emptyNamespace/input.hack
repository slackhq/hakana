namespace A {
    function foo(): void {

    }

    final class Bar {

    }
}
namespace {
    A\foo();
    \A\foo();

    (new A\Bar());
}