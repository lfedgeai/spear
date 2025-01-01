from setuptools import find_packages, setup

setup(
    name="spear",
    version="0.0.1",
    description="Spear Python SDK",
    author="Wilson Wang",
    author_email="wilson.wang@bytedance.com",
    license="MIT",
    python_requires=">=3.6",
    packages=find_packages(include=["spear", "spear.*"]),
    include_package_data=True,
    #dependencies
    install_requires=[
        "dataclasses-json",
        "flatbuffers",
        "numpy",
    ],
    # packages for building
    setup_requires=[
        "setuptools",
        "wheel",
        "pytest",
    ],
)
