namespace A {
    function foo(): void {

    }

    class Bar {

    }
}
namespace {
    A\foo();
    \A\foo();

    (new A\Bar);
}