<<Hakana\SecurityAnalysis\IgnorePathIfTrue()>>
function is_dev(): bool {
    return rand(0, 1) ? true : false;
}

function foo(): void {
    if (is_dev()) {
        $a = $_GET['a'];
        echo $a;
    }
}