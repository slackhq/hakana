final class S {
    const A = 0;
    const B = 1;
    const C = 2;
}

function foo(vec_or_dict $arr) : void {
    /* HAKANA_FIXME[PossiblyUndefinedIntArrayOffset] */
    switch ($arr[S::A]) {
        case S::B:
        case S::C:
        break;
    }
}