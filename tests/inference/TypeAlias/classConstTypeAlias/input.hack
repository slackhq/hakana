abstract class A {
  abstract const type T;
}

final class B extends A {
    const type T = vec<Exception>;
}

function foo(B::T $arr): void {
    foreach ($arr as $v) {
        echo $v;
    }
}