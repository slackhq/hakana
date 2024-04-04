function foo(string $str, vec<string> $strs): void {
    foreach ($strs as $str) { // error on this line
    	echo $str;
    }
    echo $str;
}

function bar(vec<string> $strs): void {
	$str = '';
    foreach ($strs as $str) { // this is fine
    	echo $str;
    }
    echo $str;
}

function baz(vec<string> $strs): void {
    foreach (vec['a', 'b', 'c'] as $str) {
        echo $str;
    }
    foreach ($strs as $str) { // this is also fine
        echo $str;
    }
}