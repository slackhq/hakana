type foo = shape("id" => int);

type thing<T> = shape('type' => typename<T>);

function foo(thing<foo> $thing): typename<foo> {
    return $thing['type'];
}