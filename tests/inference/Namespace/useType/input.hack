namespace Foo\Bar {
  class A {
    public static function bar(): void {}
  }
}

namespace A {
  class B {
    public static function foo(): void {}
  }
}

namespace {
  use type Foo\Bar\A;
  
  function test(): void {
    A::bar();
    A\B::foo();
  }
}