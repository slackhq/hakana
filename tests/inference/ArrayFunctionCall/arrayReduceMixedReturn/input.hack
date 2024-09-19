$arr = vec[2, 3, 4, 5];

$direct_closure_result = array_reduce(
    $arr,
    (int $carry, int $item) ==> {
        /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
        return (HH\global_get('_GET') as dict<_, _>)["boo"];
    },
    1
);