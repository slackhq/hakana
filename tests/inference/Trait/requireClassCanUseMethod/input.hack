abstract class A {
  public function getStr(): ?string {
    return rand(0, 1) ? "a" : null;
  }
}

trait T1 {
  require class B;

  public string $p;

  public function foo(): string {
    return $this->getStr();
  }

  public function callsMethodFromB(): string {
    var_dump($this->b_prop);
    return $this->someMethod();
  }
}

final class B extends A {
  use T1;

  public string $b_prop;

  <<__Override>>
  public function getStr(): string {
    return "a";
  }

  public function someMethod(): string {
    return "test";
  }
}

function foo(): void {
  $b = new B();
  $b->foo();
  var_dump($b->p);
  var_dump($b->b_prop);
}
