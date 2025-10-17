namespace N {
    final class A {
        public const VALID = 'valid';
        public static string $foo = 'valid';
    }

    function test(): string {
        $s = A::VALID . 'bar' . A::class . 'baz' . A::$foo;
        $s .= \sprintf("%d %s", 5, A::class);
        $s .= \HH\Lib\Str\format("foo %s", \B::class);
        return $s;
    }
}

namespace {
    final class B {}
    function test(): string {
        return B::class . "test" . N\A::class;
    }
}
