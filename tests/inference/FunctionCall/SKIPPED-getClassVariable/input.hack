interface I {}
class C implements I {}

function foo(): void {
    $c_instance = new C();
    (new Props())->arr[] = get_class($c_instance);
}

class Props {
    public vec<classname<I>> $arr = vec[];
}
