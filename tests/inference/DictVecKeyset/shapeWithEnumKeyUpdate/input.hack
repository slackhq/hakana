enum A: string {
    ONE = 'one';
    TWO = 'two';
    THREE = 'three';
}

type myshape = shape(
    'foo' => shape(
        A::ONE => int,
        A::TWO => string,
        A::THREE => bool,
    )
);

function foo(myshape $s): myshape {
    $s['foo'][A::ONE] = 5;
    return $s;
}