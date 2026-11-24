namespace Foo;

final class A {
    public function a(): string {
        return __METHOD__;
    }
    public function b(bool $x): string {
        if ($x) {
            return __METHOD__;
        } else {
            return __METHOD__;
        }
    }
    public function c(): string {
        $f = () ==> 1;
        $f();
        return __METHOD__;
    }
    public function d(): string {
        return __METHOD__.' '.__LINE__.' '.__CLASS__.' '.__NAMESPACE__.' '.__FUNCTION__;
    }
}

function f(): string {
    return __METHOD__;
}
