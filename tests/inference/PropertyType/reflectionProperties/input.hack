class Foo {
}

$a = new \ReflectionMethod(Foo::class, "__construct");

echo $a->name . " - " . $a->class;