function foo(string ...$strs) {
    foreach ($strs as $str) {
        echo $str;
    }
}

function bar(string ...$strs) {
    foo(...$strs);
}