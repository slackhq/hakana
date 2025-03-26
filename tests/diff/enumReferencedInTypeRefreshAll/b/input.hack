enum Foo: string as string {
    BAR = 'bar';
    BAZ = 'baz';
}

type myshape_t = shape(
    ?'a' => Foo,
    'b' => string,
);

function takesShape(myshape_t $shape): void {
    echo $shape['b'];
}
