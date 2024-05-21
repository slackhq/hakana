interface F {
    public function m(): this;
}

abstract class G implements F {}

final class H extends G {
    public function m(): F {
        return $this;
    }
}

function f1(F $f) : void {
    $f->m()->m();
}

function f2(G $f) : void {
    $f->m()->m();
}

function f3(H $f) : void {
    $f->m()->m();
}