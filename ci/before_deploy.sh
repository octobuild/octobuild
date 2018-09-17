#!/bin/bash -ex

if [ "${TRAVIS_OS_NAME}" = "linux" ]; then
    sed -i "s/__SUBJECT__/${BINTRAY_USER}/g" bintray-descriptor.json
    sed -i "s/__REPO_SLUG__/${TRAVIS_REPO_SLUG//\//\\/}/g" bintray-descriptor.json
    sed -i "s/__VERSION__/${TRAVIS_TAG}/g" bintray-descriptor.json
fi
