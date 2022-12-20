<<Hakana\SecurityAnalysis\IgnorePathIfTrue()>>
function is_dev(): bool {
    return rand(0, 1) ? true : false;
}

function foo(): void {
    if (is_dev()) {
        /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
        $a = $_GET['a'] as string;
        echo $a;
    }
}