final class C {
    public int $prop = 0;
}

function modify(readonly C $c): void {
    $c->prop = 1;
}
