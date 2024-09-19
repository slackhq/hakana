<<Hakana\SecurityAnalysis\IgnorePathIfTrue()>>
function is_dev(): bool {
    return rand(0, 1) ? true : false;
}

function foo(): void {
    if (is_dev()) {
        /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
        $a = (HH\global_get('_GET') as dict<_, _>)['a'] as string;
        echo $a;
    }
}