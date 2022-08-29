class A {}
class AChild extends A {
    public function foo(): string {
        return "cool";
    }
}

final class Foo<<<__Enforceable>> reify Tin as A, Tout as A> {
  public function __construct(
    private (function(Tin): Tout) $fn,
  ) {
  }

  public function bar(Tin $in): Tout {
    return ($this->fn)($in);
  }
}

function takesAChild(AChild $a): AChild {
    return $a;
}

$a = new Foo<AChild, _>(($a) ==> takesAChild($a));
$b = $a->bar();
echo $b->foo();