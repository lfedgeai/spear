from setuptools import find_packages, setup
import subprocess


def get_version():
    try:
        result = subprocess.check_output(['git', 'describe', '--tags', '--always', '--dirty'],
                                         stderr=subprocess.STDOUT)
        result = result.decode('utf-8').strip()
        # if it is a dirty version, change -dirty to +dirty
        if result.endswith('-dirty'):
            result = result[:-6] + '+dirty'
        return result
    except subprocess.CalledProcessError as e:
        print(f"Git command failed with error: {e.output.decode('utf-8')}")
    except FileNotFoundError:
        print("Git executable not found.")
    return '0.0.0'


setup(
    name="spear",
    version=get_version(),
    description="Spear Python SDK",
    author="Wilson Wang",
    author_email="wilson.wang@bytedance.com",
    license="MIT",
    python_requires=">=3.6",
    packages=find_packages(include=["spear", "spear.*"]),
    include_package_data=True,
    # dependencies
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
