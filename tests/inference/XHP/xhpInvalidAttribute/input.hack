use type Facebook\XHP\HTML\link;

function foo(string $a): XHPChild {
    return <link rel={"foo"} bar={"baz"} />;
}