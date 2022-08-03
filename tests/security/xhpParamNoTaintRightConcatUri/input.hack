use type Facebook\XHP\HTML\a;

function foo(string $url) {
    return <a href={"https://slack.com/foo/" . $url}>I'm a normal link</a>;
}

foo($_GET['url']);

function bar(string $url) {
    return <a href={"https://slack.com/foo/$url"}>I'm a normal link</a>;
}

bar($_GET['url']);