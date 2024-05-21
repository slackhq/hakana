namespace Classes {
  use Traits\T;

  final class A {}

  final class B {
    use T;
  }
}

namespace Traits {
  use Classes\A;

  trait T {
    public function getA() : A {
      return new A();
    }
  }
}

namespace {
    $a = (new Classes\B())->getA();
}