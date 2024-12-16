package main

import (
	"github.com/lfedgeai/spear/worker"
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
	"github.com/spf13/cobra"
)

type WorkerConfig struct {
	Addr string
	Port string
}

var (
	execRtTypeStr    string
	execWorkloadName string
	execReqMethod    string
	execReqPayload   string
	execIPAddress	 string
	execVerbose      bool
	execDebug        bool
)

func NewRootCmd() *cobra.Command {
	var rootCmd = &cobra.Command{
		Use:   "worker",
		Short: "Worker is the command line tool for the worker",
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
				"Docker":  task.TaskTypeDocker,
				"Process": task.TaskTypeProcess,
				"Dylib":   task.TaskTypeDylib,
				"Wasm":    task.TaskTypeWasm,
			}

			if execWorkloadName == "" {
				log.Errorf("Invalid workload name %s", execWorkloadName)
				return
			}
			if execReqMethod == "" {
				log.Errorf("Invalid request method %s", execReqMethod)
				return
			}

			// check if the workload type is valid
			if rtType, ok := validChoices[execRtTypeStr]; !ok {
				log.Errorf("Invalid runtime type %s", execRtTypeStr)
			} else {
				log.Infof("Executing workload %s with runtime type %v", execWorkloadName, rtType)
				// set log level
				if execVerbose {
					worker.SetLogLevel(log.DebugLevel)
				}

				// create config
				config := worker.NewExecWorkerConfig(execDebug)
				w := worker.NewWorker(config)
				w.Initialize()

				// lookup task id
				execWorkloadId, err := w.LookupTaskId(execWorkloadName)
				if err != nil {
					log.Errorf("Error looking up task id: %v", err)
					// print available tasks
					tasks := w.ListTasks()
					log.Infof("Available tasks: %v", tasks)
					w.Stop()
					return
				}

				res, err := w.ExecuteTask(execWorkloadId, rtType, true, execReqMethod, execReqPayload, execIPAddress)
				if err != nil {
					log.Errorf("Error executing workload: %v", err)
				}
				log.Debugf("Workload execution result: %v", res)
				w.Stop()
				// TODO: implement workload execution
			}
		},
	}

	// workload id
	execCmd.PersistentFlags().StringVarP(&execWorkloadName, "name", "n", "", "workload name")
	// workload type, a choice of Docker, Process, Dylib or Wasm
	execCmd.PersistentFlags().StringVarP(&execRtTypeStr, "type", "t", "Docker", "type of the workload")
	// workload request payload
	execCmd.PersistentFlags().StringVarP(&execReqMethod, "method", "m", "handle", "default method to invoke")
	execCmd.PersistentFlags().StringVarP(&execReqPayload, "payload", "p", "", "request payload")
	// verbose flag
	execCmd.PersistentFlags().BoolVarP(&execVerbose, "verbose", "v", false, "verbose output")
	// debug flag
	execCmd.PersistentFlags().BoolVarP(&execDebug, "debug", "d", false, "debug mode")
	// host ip flag
	execCmd.PersistentFlags().StringVarP(&execIPAddress, "ip", "i", "", "input host ip")
	rootCmd.AddCommand(execCmd)

	var serveCmd = &cobra.Command{
		Use:   "serve",
		Short: "Start the worker server",
		Run: func(cmd *cobra.Command, args []string) {
			// parse flags
			addr, _ := cmd.Flags().GetString("addr")
			port, _ := cmd.Flags().GetString("port")
			verbose, _ := cmd.Flags().GetBool("verbose")
			paths, _ := cmd.Flags().GetStringArray("search-path")
			debug, _ := cmd.Flags().GetBool("debug")

			// set log level
			if verbose {
				worker.SetLogLevel(log.DebugLevel)
			}

			// create config
			config := worker.NewServeWorkerConfig(addr, port, paths,
				debug)
			w := worker.NewWorker(config)
			w.Initialize()
			w.StartServer()
		},
	}
	// addr flag
	serveCmd.PersistentFlags().StringP("addr", "a", "localhost", "address of the server")
	// port flag
	serveCmd.PersistentFlags().StringP("port", "p", "8080", "port of the server")
	// verbose flag
	serveCmd.PersistentFlags().BoolP("verbose", "v", false, "verbose output")
	// search path
	serveCmd.PersistentFlags().StringArrayP("search-path", "L", []string{},
		"search path list for the worker")
	// debug flag
	serveCmd.PersistentFlags().BoolP("debug", "d", false, "debug mode")
	rootCmd.AddCommand(serveCmd)

	return rootCmd
}

func main() {
	NewRootCmd().Execute()
}
