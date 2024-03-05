<<__EntryPoint>>
function main() {
    bar(shape('a' => 10, 'b' => 'a'));
}

function bar(foo_t $foo) {
    echo Shapes::idx($foo, 'b', null);
}