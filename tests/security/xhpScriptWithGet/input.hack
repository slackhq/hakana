use type Facebook\XHP\HTML\script;

function foo(string $data) {
    return <script>{$data}</script>;
}

foo($_GET['url']);
