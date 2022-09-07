type foo_t = shape('id' => string, 'name' => string);

function foo(dict<int, foo_t> $foos): void {
    foreach ($foos as $foo) {
        $name = $foo['name'];
        echo $name;
        Shapes::removeKey(inout $foo, 'name');
    }
}