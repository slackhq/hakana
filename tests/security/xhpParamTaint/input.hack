use type Facebook\XHP\HTML\a;

function foo(string $url) {
    return <a href={$url}>I'm a normal link</a>;
}

foo(HH\global_get('_GET')['url']);