package main

import (
	"strings"

	"github.com/lfedgeai/spear/pkg/common"
	"github.com/lfedgeai/spear/pkg/spear/proto/transport"
	spearlet "github.com/lfedgeai/spear/spearlet"
	"github.com/lfedgeai/spear/spearlet/task"
	log "github.com/sirupsen/logrus"
	"github.com/spf13/cobra"

	"os"
)

var (
	execRtTypeStr    string
	execWorkloadName string
	execProcFileName string
	execReqMethod    string
	execReqPayload   string

	runStartBackendServices bool
	runSpearAddr            string
	runSearchPaths          []string
	runVerbose              bool
	runDebug                bool

	serveAddr string
	servePort string

	// Cert & Key files can be generated for testing using command
	// openssl req -x509 -newkey rsa:2048 -keyout server.key -out server.crt -days 365 -nodes
	serveCertFile string
	serveKeyFile  string
)

func isValidSearchPaths(paths []string) bool {
	for _, path := range paths {
		// make sure it is a path, not a file
		if s, err := os.Stat(path); err == nil {
			if !s.IsDir() {
				log.Errorf("Invalid search path %s", path)
				return false
			}
		} else {
			log.Errorf("Invalid search path %s", path)
			return false
		}
	}
	return true
}

func NewRootCmd() *cobra.Command {
	var rootCmd = &cobra.Command{
		Use:   "spearlet",
		Short: "spearlet is the command line tool for the SPEAR spearlet",
		Run: func(cmd *cobra.Command, args []string) {
			cmd.Help()
		},
	}

	// exec subcommand
	var execCmd = &cobra.Command{
		Use:   "exec",
		Short: "Execute a workload",
		Run: func(cmd *cobra.Command, args []string) {
			var validChoices = map[string]task.TaskType{
				"docker":  task.TaskTypeDocker,
				"process": task.TaskTypeProcess,
				"dylib":   task.TaskTypeDylib,
				"wasm":    task.TaskTypeWasm,
				"unknown": task.TaskTypeUnknown,
			}

			if execWorkloadName == "" && execProcFileName == "" {
				log.Errorf("Invalid workload name %s", execWorkloadName)
				return
			} else if execProcFileName != "" && execWorkloadName != "" {
				log.Error("Cannot specify both workload name and process filename at the same time")
				return
			}
			if execReqMethod == "" {
				log.Errorf("Invalid request method %s", execReqMethod)
				return
			}
			if runSpearAddr == "" {
				runSpearAddr = common.SpearPlatformAddress
			}
			if !isValidSearchPaths(runSearchPaths) {
				log.Errorf("Invalid search paths")
				return
			}

			// check if the workload type is valid
			if rtType, ok := validChoices[strings.ToLower(execRtTypeStr)]; !ok {
				log.Errorf("Invalid runtime type %s", execRtTypeStr)
			} else {
				log.Infof("Executing workload %s with runtime type %v",
					execWorkloadName, rtType)
				// set log level
				if runVerbose {
					spearlet.SetLogLevel(log.DebugLevel)
				}

				// create config
				config := spearlet.NewExecSpearletConfig(runDebug, runSpearAddr,
					runSearchPaths, runStartBackendServices)
				w := spearlet.NewSpearlet(config)
				w.Initialize()

				defer func() {
					w.Stop()
				}()

				var funcId int64 = -1
				var funcName string = ""
				var err error
				if execWorkloadName != "" {
					// lookup task id
					funcId, err = w.LookupTaskId(execWorkloadName)
					if err != nil {
						log.Errorf("Error looking up task id: %v", err)
						// print available tasks
						tasks := w.ListTasks()
						log.Infof("Available tasks: %v", tasks)
						return
					}
				} else if execProcFileName != "" {
					funcName = execProcFileName
				} else {
					log.Errorf("Invalid workload name %s", execWorkloadName)
					return
				}

				t, outData, outStream, err := w.ExecuteTask(funcId, funcName, rtType,
					execReqMethod, execReqPayload, nil)
				if err != nil {
					log.Errorf("Error executing workload: %v", err)
					return
				}

				if outData != "" {
					log.Infof("Workload execution output: %v", outData)
				}
				if outStream != nil {
					// print out stream
					log.Infof("Workload execution output stream: %v", outStream)
					for msg := range outStream {
						log.Infof("%v", msg)
					}
				}

				log.Infof("Terminating task %v", t)
				// terminate the task by sending a signal
				if err := w.CommunicationManager().SendOutgoingRPCSignal(t,
					transport.SignalTerminate,
					[]byte{}); err != nil {
					log.Warnf("Error: %v", err)
				}
				if err := t.Stop(); err != nil {
					log.Warnf("Error stopping task: %v", err)
				}
			}
		},
	}

	// workload name
	execCmd.PersistentFlags().StringVarP(&execWorkloadName, "name", "n", "",
		"workload name. Cannot be used with process workload filename at the same time")
	// workload filename
	execCmd.PersistentFlags().StringVarP(&execProcFileName, "file", "f", "",
		"process workload filename. Only valid for process type workload")
	// workload type, a choice of Docker, Process, Dylib or Wasm
	execCmd.PersistentFlags().StringVarP(&execRtTypeStr, "type", "t", "unknown",
		"type of the workload. By default, it is unknown and the spearlet will try to determine the type.")
	// workload request payload
	execCmd.PersistentFlags().StringVarP(&execReqMethod, "method", "m", "handle",
		"default method to invoke")
	execCmd.PersistentFlags().StringVarP(&execReqPayload, "payload", "p", "", "request payload")
	rootCmd.AddCommand(execCmd)

	var serveCmd = &cobra.Command{
		Use:   "serve",
		Short: "Start the spearlet server",
		Run: func(cmd *cobra.Command, args []string) {
			// set log level
			if runVerbose {
				spearlet.SetLogLevel(log.DebugLevel)
			}

			if runSpearAddr == "" {
				runSpearAddr = common.SpearPlatformAddress
			}
			if !isValidSearchPaths(runSearchPaths) {
				log.Errorf("Invalid search paths")
				return
			}

			// create config
			config, err := spearlet.NewServeSpearletConfig(serveAddr, servePort, runSearchPaths,
				runDebug, runSpearAddr, serveCertFile, serveKeyFile, runStartBackendServices)
			if err != nil {
				log.Errorf("Error creating spearlet config: %v", err)
				return
			}
			w := spearlet.NewSpearlet(config)
			w.Initialize()
			w.StartProviderService()
			w.StartServer()
		},
	}
	// addr flag
	serveCmd.PersistentFlags().StringVarP(&serveAddr, "addr", "a", "localhost",
		"address of the server")
	// port flag
	serveCmd.PersistentFlags().StringVarP(&servePort, "port", "p", "8080", "port of the server")
	// cert file flag
	serveCmd.PersistentFlags().StringVarP(&serveCertFile, "ssl-cert", "c", "", "SSL certificate file")
	// key file flag
	serveCmd.PersistentFlags().StringVarP(&serveKeyFile, "ssl-key", "k", "", "SSL key file")
	rootCmd.AddCommand(serveCmd)

	// spear platform address for workload to connect
	rootCmd.PersistentFlags().StringVarP(&runSpearAddr, "spear-addr", "s", os.Getenv("SPEAR_RPC_ADDR"),
		"SPEAR platform address for workload RPC")
	// search path
	rootCmd.PersistentFlags().StringArrayVarP(&runSearchPaths, "search-path", "L", []string{},
		"search path list for the spearlet")
	// verbose flag
	rootCmd.PersistentFlags().BoolVarP(&runVerbose, "verbose", "v", false, "verbose output")
	// debug flag
	rootCmd.PersistentFlags().BoolVarP(&runDebug, "debug", "d", false, "debug mode")
	// backend service
	rootCmd.PersistentFlags().BoolVarP(&runStartBackendServices, "backend-services", "b", false,
		"start backend services")
	// version flag
	rootCmd.Version = common.Version
	return rootCmd
}

func main() {
	NewRootCmd().Execute()
}
