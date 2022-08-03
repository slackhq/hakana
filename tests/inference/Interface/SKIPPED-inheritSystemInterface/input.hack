interface I extends \RecursiveIterator {}

function f(I $c): void {
    $c->current();
}