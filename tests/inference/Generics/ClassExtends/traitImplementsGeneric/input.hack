interface I<T> {
    public function takesVec(vec<T> $v): void;
    public function getVec(): vec<T>;
}

trait MyTrait<T> implements I<T> {
}

final class C {}

abstract class Base {
    public function getVec(): vec<string> {
        return vec[];
    }
}

final class Concrete extends Base {
    use MyTrait<string>;

    <<__Override>>
    public function takesVec(vec<string> $v): void {}
}

function take_concrete(Concrete $c): void {
    $c->takesVec($c->getVec());
}