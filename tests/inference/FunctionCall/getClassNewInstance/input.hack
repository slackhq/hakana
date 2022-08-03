interface I {}
class C implements I {}

class Props {
    public vec<classname<I>> $arr = vec[];
}

(new Props)->arr[] = get_class(new C);