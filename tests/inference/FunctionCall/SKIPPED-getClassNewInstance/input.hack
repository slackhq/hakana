interface I {}
final class C implements I {}

final class Props {
    public vec<classname<I>> $arr = vec[];
}

(new Props())->arr[] = get_class(new C());