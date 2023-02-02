abstract class Base {}
class A extends Base {}
class AChild extends A {}
class B extends Base {
    public string $s = "";
}

function foo(Base $base): void {
    if ((!$base is A || $base is AChild) && $base is B && rand(0, 1)) {
        echo $base->s;
    }
}

function bar(Base $base): void {
    if (!$base is A || $base is AChild) {
        if ($base is B && rand(0, 1)) {
            echo $base->s;
        }
    }
}