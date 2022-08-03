class A {}
class B {}

$foo = vec[
    A::class,
    B::class
];

foreach ($foo as $class) {
    if ($class === A::class) {}
}