abstract class A {
  abstract const type T;
}

final class B extends A {
    const type T = vec<Exception>;

    public function bar(): void {}
}

function foo(B::T $arr): void {
    foreach ($arr as $v) {
        echo $v;
    }
}

<<__EntryPoint>>
function main(): void {
    foo(vec[new Exception("bad")]);
    (new B())->bar();
}