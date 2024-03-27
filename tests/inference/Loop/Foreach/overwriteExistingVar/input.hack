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
