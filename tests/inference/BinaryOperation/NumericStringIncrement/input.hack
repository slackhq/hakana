function scope(vec_or_dict $a): num {
    $offset = array_search("foo", $a);
    if(is_numeric($offset)){
        return $offset++;
    }
    else{
        return 0;
    }
}