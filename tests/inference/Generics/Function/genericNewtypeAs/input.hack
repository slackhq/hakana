function foo(my_dict $dict): dict<string, string> {
    return Dict\map($dict, ($field_value) ==> {
        return $field_value;
    });
}