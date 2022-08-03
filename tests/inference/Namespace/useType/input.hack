namespace Foo\Bar {
  class A {
    static function bar(): void {}
  }
}

namespace A {
  class B {
    static function foo(): void {}
  }
}

namespace {
  use type Foo\Bar\A;
  
  function test(): void {
    A::bar();
    A\B::foo();
  }
}