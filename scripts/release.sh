#!/usr/bin/env sh

name=$1
release_dir="$PWD/$2"
target_list="${@:3}"

echo "Tarballing all targets to $release_dir."
mkdir -p "$release_dir"
cd "target/" || exit

for target in $target_list
do
    echo "Tarballing $target..."
    cd "$target/release" && tar -czf "$name-$target.tar.gz" "$name" && mv "$name-$target.tar.gz" "$release_dir" && cd ../..
done

echo "done."
