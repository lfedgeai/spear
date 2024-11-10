from setuptools import setup, find_packages

setup(
    name="spear",
    version="0.1",
    description="Spear Python SDK",
    author="Wilson Wang",
    author_email="wilson.wang@bytedance.com",
    license="MIT",
    python_requires=">=3.6",
    packages=find_packages(include=["spear", "spear.*"]),
    include_package_data=True,
)
