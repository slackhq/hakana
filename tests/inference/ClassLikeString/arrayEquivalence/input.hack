final class A {}
final class B {}

$foo = vec[
    A::class,
    B::class
];

foreach ($foo as $class) {
    if ($class === A::class) {}
}