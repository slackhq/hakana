$bar = vec["a", 2];
list($a, $b) = $bar;

hakana_expect_type<string>($a);
hakana_expect_type<int>($b);