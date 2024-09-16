final class A {
    public static function foo<T>(
		typename<T> $shape,
    ): T {

    }
}

type my_shape = shape(
    'a' => bool,
);

function main(): void {
    A::foo<my_shape>(my_shape::class);
}